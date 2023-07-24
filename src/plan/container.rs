use std::{
    collections::{hash_map::RandomState, hash_set::SymmetricDifference, HashMap, HashSet},
    ops::Deref,
    path::{Path, PathBuf},
    time::SystemTime,
};

use async_compat::CompatExt;
use async_recursion::async_recursion;
use base64::{prelude::BASE64_URL_SAFE_NO_PAD, Engine};
use maplit::hashmap;
use miette::Context;
use podman_api::{
    models::{InspectAdditionalNetwork, InspectMount, ListContainer, NamedVolume, Namespace, PortMapping},
    opts::{ContainerCreateOpts, ContainerListFilter, ContainerListOpts},
    Podman,
};
use serde::{Deserialize, Serialize};

use super::{
    image::ResolvedImageRef, network::ResolvedNetworkRef, secret::ResolvedSecretRef, volume::ResolvedVolumeRef, PostAction,
    StepContext,
};
use crate::{
    parse::model::{ParsedContainerInject, ParsedContainerMountType, ParsedProtocol},
    utils::{BodyWriter, IntoDiagnosticShorthand, XTug},
};

#[derive(Clone, Debug)]
pub struct ContainerAction {
    pub name: String,
    pub command: Option<Vec<String>>,
    pub image: ResolvedImageRef,
    pub ports: Vec<ContainerActionPort>,
    pub injects: Vec<ParsedContainerInject>,
    pub networks: Vec<ContainerActionNetwork>,
    pub mounts: Vec<ContainerActionMount>,
    pub secrets: Vec<ContainerActionSecret>,
}

#[derive(Clone, Debug)]
pub struct ContainerActionPort {
    pub container: u16,
    pub host: u16,
    pub protocol: ParsedProtocol,
}

#[derive(Clone, Debug)]
pub struct ContainerActionNetwork {
    pub resolved: ResolvedNetworkRef,
    pub aliases: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct ContainerActionMount {
    pub kind: ParsedContainerMountType,
    pub name_ref: ResolvedVolumeRef,
    pub destination: String,
}

#[derive(Clone, Debug)]
pub struct ContainerActionSecret {
    pub name_ref: ResolvedSecretRef,
    pub target: String,
}

pub async fn execute(ctx: &StepContext, action: ContainerAction) -> miette::Result<()> {
    let mut fingerprint_cache = HashMap::new();
    let mut secret_fulls = None;

    let remote_containers = remote_containers_query(&ctx.service, ctx.group.clone(), action.name.clone()).await?;

    if remote_containers.is_empty() {
        create_container(ctx, &action, fingerprint_cache, secret_fulls)
            .await
            .wrap_err("creating container")?;
        return Ok(());
    }

    let first_container = remote_containers.first().unwrap();
    let first_container_inspect = ctx
        .service
        .containers()
        .get(first_container.id.as_ref().unwrap())
        .inspect()
        .await
        .d()?;

    if remote_containers.len() == 1
        && first_container_inspect.image == Some(ctx.resolved_images.lock()[&action.image].to_string())
        && first_container.command == action.command
        && check_port_mappings(&action.ports, first_container.ports.as_deref().unwrap_or_default())
        && check_network_mappings(
            ctx,
            &action.networks,
            &first_container_inspect.network_settings.unwrap().networks.unwrap_or_default(),
        )
        && check_mount_mappings(ctx, &action.mounts, &first_container_inspect.mounts.unwrap_or_default())
    // todo: secret sync check
    {
        let labels = first_container.labels.clone().unwrap_or_default();

        let injects: Option<HashMap<PathBuf, InjectNode>> = labels
            .get(XTug::InjectFingerprint.as_ref())
            .and_then(|compare| BASE64_URL_SAFE_NO_PAD.decode(compare).ok())
            .and_then(|compare| rmp_serde::from_slice(&compare).ok());

        let inject_names_match = match &injects {
            Some(entries) => {
                let requested: HashSet<PathBuf, RandomState> =
                    HashSet::from_iter(action.injects.iter().map(|inject| PathBuf::clone(&inject.at)));
                let actual: HashSet<PathBuf, RandomState> = HashSet::from_iter(entries.keys().cloned());

                requested.difference(&actual).next().is_none() && actual.difference(&requested).next().is_none()
            }
            None if action.injects.is_empty() => true,
            None => false,
        };

        let mut inject_fingerprints_match = true;
        if let Some(injects) = &injects {
            if inject_names_match {
                for inject in &action.injects {
                    let (updated_fingerprint, bad) = inject_fingerprint(ctx, inject, injects.get(inject.at.deref()))
                        .await
                        .wrap_err("pre-computing inject fingerprint")?;
                    fingerprint_cache.insert(inject.at.deref().clone(), updated_fingerprint);
                    if bad {
                        inject_fingerprints_match = false;
                        break;
                    }
                }
            }
        }

        let mut secret_fingerprint_matches = true;
        let existing_fingerprint = labels
            .get(XTug::SecretFingerprint.as_ref())
            .and_then(|compare| BASE64_URL_SAFE_NO_PAD.decode(compare).ok())
            .and_then(|compare| rmp_serde::from_slice::<Vec<SecretFingerprint>>(&compare).ok());
        match (existing_fingerprint, action.secrets.is_empty()) {
            (None, true) => {}
            (None, false) | (Some(_), true) => {
                secret_fingerprint_matches = false;
            }
            (Some(remote_fingerprint), false) => {
                let (fulls, bad) = secret_fingerprint(ctx, &action.secrets, Some(&remote_fingerprint)).await?;
                secret_fulls = Some(fulls);
                secret_fingerprint_matches = !bad;
            }
        }

        if inject_names_match && inject_fingerprints_match && secret_fingerprint_matches {
            if first_container.state.as_deref() != Some("running") {
                let id = first_container.id.as_ref().unwrap();
                let container = ctx.service.containers().get(id);
                container
                    .start(None)
                    .await
                    .d()
                    .wrap_err_with(|| format!("starting container {id}"))?;
            }
            return Ok(());
        }
    }

    for container in remote_containers {
        let id = container.id.unwrap();
        let container = ctx.service.containers().get(&id);
        container.stop(&Default::default()).await.d()?;
        ctx.backtrack.lock().push(PostAction::RestartContainer { id: id.clone() });
        ctx.finalize.lock().push(PostAction::DeleteContainer { id });
    }

    create_container(ctx, &action, fingerprint_cache, secret_fulls)
        .await
        .wrap_err("creating container")?;

    Ok(())
}

pub async fn remote_containers_query(service: &Podman, group: String, name: String) -> miette::Result<Vec<ListContainer>> {
    service
        .containers()
        .list(
            &ContainerListOpts::builder()
                .filter([
                    ContainerListFilter::LabelKeyVal(XTug::Group.to_string(), group),
                    ContainerListFilter::LabelKeyVal(XTug::Name.to_string(), name),
                ])
                .build(),
        )
        .await
        .d()
}

async fn create_container(
    ctx: &StepContext,
    action: &ContainerAction,
    fingerprint_cache: HashMap<PathBuf, InjectNode>,
    secret_fulls: Option<Vec<FullSecret>>,
) -> miette::Result<()> {
    let image = ctx.resolved_images.lock()[&action.image].to_string();
    let mut inject_fingerprints = fingerprint_cache;
    for inject in &action.injects {
        if !inject_fingerprints.contains_key(inject.at.deref()) {
            inject_fingerprints.insert(
                inject.at.deref().clone(),
                inject_fingerprint(ctx, inject, None)
                    .await
                    .wrap_err_with(|| format!("calculating fingerprint inline for {inject:?}"))?
                    .0,
            );
        }
    }
    let inject_fingerprints = BASE64_URL_SAFE_NO_PAD.encode(rmp_serde::to_vec(&inject_fingerprints).d()?);

    let mut opts = ContainerCreateOpts::builder()
        .image(image)
        .portmappings(action.ports.iter().map(|port| PortMapping {
            container_port: Some(port.container),
            host_ip: None,
            host_port: Some(port.host),
            protocol: Some(port.protocol.to_string()),
            range: None,
        }))
        .net_namespace(Namespace {
            nsmode: Some("bridge".to_string()),
            value: None,
        })
        .networks(action.networks.iter().map(|network| {
            (ctx.resolved_networks.lock()[&network.resolved].to_string(), hashmap! {
                "aliases" => network.aliases.clone()
            })
        }));

    let mut volumes = Vec::new();
    for mount in &action.mounts {
        let name = ctx.resolved_volumes.lock()[&mount.name_ref].clone();
        volumes.push(NamedVolume {
            dest: Some(mount.destination.clone()),
            is_anonymous: Some(false),
            name: Some(name),
            options: None,
        });
    }

    opts = opts.volumes(volumes);

    if let Some(command) = &action.command {
        opts = opts.command(command);
    }

    let secret_fulls = match secret_fulls {
        Some(s) => s,
        None => secret_fingerprint(ctx, &action.secrets, None).await?.0,
    };

    opts = opts.secret_env(
        secret_fulls
            .iter()
            .map(|secret| (secret.target.to_string(), secret.id.clone())),
    );
    let print = BASE64_URL_SAFE_NO_PAD.encode(rmp_serde::to_vec(&secret_print_from_fulls(&secret_fulls)).d()?);

    opts = opts.labels([
        (XTug::Group.to_string(), ctx.group.clone()),
        (XTug::Name.to_string(), action.name.to_string()),
        (XTug::InjectFingerprint.to_string(), inject_fingerprints),
        (XTug::SecretFingerprint.to_string(), print),
    ]);

    let new_container = ctx.service.containers().create(&opts.build()).await.d()?;
    let container = ctx.service.containers().get(&new_container.id);

    let cwd = std::env::current_dir().d()?;
    for inject in &action.injects {
        let (writer, body) = BodyWriter::new();
        let copy_task = tokio::spawn({
            let at = inject.at.deref().clone();
            let container = ctx.service.containers().get(&new_container.id);
            async move { container.copy_to(at, body).await }
        });
        let mut archive = async_tar::Builder::new(writer);
        archive_append(&mut archive, cwd.join(&ctx.root_directory).join(&inject.path), PathBuf::new()).await?;
        let writer = archive.into_inner().await.d()?;
        drop(writer);
        copy_task.await.d()?.d()?;
    }

    container.start(None).await.d()?;

    ctx.backtrack.lock().push(PostAction::DeleteContainer {
        id: container.id().to_string(),
    });

    Ok(())
}

#[async_recursion]
async fn archive_append(archive: &mut async_tar::Builder<BodyWriter>, path: PathBuf, in_archive: PathBuf) -> miette::Result<()> {
    let meta = tokio::fs::metadata(&path).await.d()?;
    if meta.is_dir() {
        let mut entries = tokio::fs::read_dir(&path).await.d()?;
        while let Some(entry) = entries.next_entry().await.d()? {
            archive_append(archive, entry.path(), in_archive.join(entry.file_name())).await?;
        }
    } else {
        let mut header = async_tar::Header::new_gnu();
        header.set_metadata(&meta);
        let mut file = tokio::fs::File::open(&path).await.d()?;
        archive
            .append_data(
                &mut header,
                {
                    if in_archive.as_os_str().is_empty() {
                        in_archive.join(path.file_name().unwrap_or_default())
                    } else {
                        in_archive
                    }
                },
                file.compat_mut(),
            )
            .await
            .d()?;
    }

    Ok(())
}

fn check_port_mappings(expected: &[ContainerActionPort], actual: &[PortMapping]) -> bool {
    let mut actual = actual.to_vec();

    for definition in expected {
        let Some((index, _)) = actual.iter().enumerate().find(|(_, mapping)| {
            mapping.container_port == Some(definition.container)
                && mapping.host_port == Some(definition.host)
                && mapping.protocol.as_deref() == Some(definition.protocol.as_str())
        }) else {
            return false;
        };
        actual.swap_remove(index);
    }

    actual.is_empty()
}

fn check_network_mappings(
    ctx: &StepContext,
    expected: &[ContainerActionNetwork],
    actual: &HashMap<String, InspectAdditionalNetwork>,
) -> bool {
    let mut actual = actual.clone();

    for definition in expected {
        match actual.remove(&ctx.resolved_networks.lock()[&definition.resolved]) {
            Some(actual) => {
                if actual.aliases.as_ref() != Some(&definition.aliases) {
                    let differences = SymmetricDifference::<&String, RandomState>::count(
                        actual
                            .aliases
                            .as_ref()
                            .map(HashSet::from_iter)
                            .unwrap_or_default()
                            .symmetric_difference(&HashSet::from_iter(&definition.aliases)),
                    );
                    return differences <= 1;
                }
            }
            None => {
                return false;
            }
        }
    }

    actual.is_empty()
}

fn check_mount_mappings(ctx: &StepContext, expected: &[ContainerActionMount], actual: &[InspectMount]) -> bool {
    let mut actual = actual.to_vec();

    for definition in expected {
        let volume = ctx.resolved_volumes.lock()[&definition.name_ref].clone();
        let index = match actual.iter().enumerate().find(|(_, mount)| {
            mount.destination.as_ref() == Some(&definition.destination)
                && mount.name.as_ref() == Some(&volume)
                && mount.destination.as_ref() == Some(&definition.destination)
        }) {
            Some((index, _)) => index,
            None => {
                return false;
            }
        };
        actual.swap_remove(index);
    }

    actual.is_empty()
}

async fn secret_fingerprint(
    ctx: &StepContext,
    secrets: &[ContainerActionSecret],
    compare: Option<&[SecretFingerprint]>,
) -> miette::Result<(Vec<FullSecret>, bool)> {
    let mut fulls = Vec::new();
    for secret in secrets {
        let secret_id = ctx.resolved_secrets.lock()[&secret.name_ref].clone();
        let info = ctx.service.secrets().get(&secret_id).inspect().await.d()?;
        let updated_at = info.updated_at.unwrap().timestamp();
        fulls.push(FullSecret {
            id: secret_id,
            target: secret.target.clone(),
            updated_at,
        });
    }

    if let Some(compare) = compare {
        let bad = secret_print_from_fulls(&fulls) != *compare;
        Ok((fulls, bad))
    } else {
        Ok((fulls, true))
    }
}

fn secret_print_from_fulls(fulls: &[FullSecret]) -> Vec<SecretFingerprint> {
    let mut prints = Vec::new();
    for full in fulls {
        prints.push(SecretFingerprint {
            id: full.id.clone(),
            updated_at: full.updated_at,
        });
    }
    prints.sort_unstable();
    prints
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
struct SecretFingerprint {
    id: String,
    updated_at: i64,
}

struct FullSecret {
    id: String,
    target: String,
    updated_at: i64,
}

async fn inject_fingerprint(
    ctx: &StepContext,
    inject: &ParsedContainerInject,
    compare: Option<&InjectNode>,
) -> miette::Result<(InjectNode, bool)> {
    let base = ctx.root_directory.join(&inject.path);
    compute_node(&base, &compare).await
}

#[async_recursion]
async fn compute_node(at: &Path, compare: &Option<&InjectNode>) -> miette::Result<(InjectNode, bool)> {
    let meta = tokio::fs::metadata(&at)
        .await
        .d()
        .wrap_err_with(|| format!("checking metadata for file at {at:?}"))?;
    if meta.is_dir() {
        let mut bad = !matches!(compare, Some(InjectNode::Directory(_)));
        let mut contents = HashMap::new();
        let mut entries = tokio::fs::read_dir(at).await.d()?;
        while let Some(entry) = entries.next_entry().await.d()? {
            let file_name = crate::utils::os_string_vec(entry.file_name());
            let compare = match compare {
                Some(InjectNode::Directory(map)) => map.get(&file_name),
                _ => None,
            };
            let (node, recurse_bad) = compute_node(&entry.path(), if bad { &None } else { &compare }).await?;
            bad |= recurse_bad;
            contents.insert(file_name, node);
        }
        Ok((InjectNode::Directory(contents), bad))
    } else {
        let current_mtime = meta
            .modified()
            .d()?
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let bad = match compare {
            Some(InjectNode::File { mtime, len }) => current_mtime != *mtime || meta.len() != *len,
            _ => true,
        };
        Ok((
            InjectNode::File {
                mtime: current_mtime,
                len: meta.len(),
            },
            bad,
        ))
    }
}

#[derive(Serialize, Deserialize, Clone)]
enum InjectNode {
    #[serde(rename = "d")]
    Directory(HashMap<Vec<u8>, InjectNode>),
    #[serde(rename = "f")]
    File {
        #[serde(rename = "m")]
        mtime: u128,
        #[serde(rename = "l")]
        len: u64,
    },
}
