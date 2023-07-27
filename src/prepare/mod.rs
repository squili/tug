pub mod diagnostics;

use std::collections::{BTreeSet, HashMap};

use miette::NamedSource;

use self::diagnostics::{read_source, DuplicateInjectPath, DuplicateName, MalformedCommand, UnknownThing};
use crate::{
    logger::Logger,
    parse::model::{ParsedContainerMount, ParsedContainerPort, ParsedDocument, ParsedExplicitContainerPort, ParsedProtocol},
    plan::{
        container::{
            ContainerAction, ContainerActionBindMount, ContainerActionNetwork, ContainerActionPort, ContainerActionSecret,
            ContainerActionVolumeMount,
        },
        garbage::GarbageAction,
        image::{ImageAction, ResolvedImageRef},
        network::{NetworkAction, ResolvedNetworkRef},
        secret::{ResolvedSecretRef, SecretAction},
        volume::{ResolvedVolumeRef, VolumeAction},
        Action, Executor,
    },
    utils::IntoDiagnosticShorthand,
};

pub fn prepare(logger: &Logger, document: ParsedDocument, executor: &mut Executor) -> miette::Result<()> {
    logger.log("Preparing plan");

    logger.log("Queueing garbage pass");
    executor.new_step(
        Action::Garbage(GarbageAction {
            container_names: document
                .containers
                .iter()
                .map(|container| container.name.to_string())
                .collect(),
        }),
        BTreeSet::new(),
    );

    logger.log("Queueing images");
    let mut image_name_to_dependency = HashMap::new();
    let mut counter = 1;
    for image in document.images {
        let resolved = ResolvedImageRef(counter);
        counter += 1;
        let step_id = executor.new_step(
            Action::Image(ImageAction {
                resolved,
                name: image.name.to_string(),
                reference: image.reference.to_string(),
                reference_span: image.reference.span().clone(),
                local: image.local,
            }),
            BTreeSet::new(),
        );
        if let Some((_, _, old_span)) =
            image_name_to_dependency.insert(image.name.to_string(), (resolved, step_id, image.name.span().clone()))
        {
            DuplicateName::from_spans(&old_span, image.name.span())?
        }
    }
    let image_name_to_dependency = image_name_to_dependency
        .into_iter()
        .map(|(name, (reference, step, _))| (name, (reference, step)))
        .collect::<HashMap<_, _>>();

    logger.log("Queueing networks");
    let mut network_name_to_dependency = HashMap::new();
    let mut counter = 1;
    for network in document.networks {
        let resolved = ResolvedNetworkRef(counter);
        counter += 1;
        let step_id = executor.new_step(
            Action::Network(NetworkAction {
                name: network.name.to_string(),
                dns_enabled: network.dns_enabled,
                internal: network.internal,
                driver: network.driver.clone(),
                resolved,
            }),
            BTreeSet::new(),
        );
        if let Some((_, _, old_span)) =
            network_name_to_dependency.insert(network.name.to_string(), (resolved, step_id, network.name.span().clone()))
        {
            DuplicateName::from_spans(&old_span, network.name.span())?
        }
    }
    let network_name_to_dependency = network_name_to_dependency
        .into_iter()
        .map(|(name, (reference, step, _))| (name, (reference, step)))
        .collect::<HashMap<_, _>>();

    logger.log("Queueing volumes");
    let mut volume_name_to_dependency = HashMap::new();
    let mut counter = 1;
    for volume in document.volumes {
        let resolved = ResolvedVolumeRef(counter);
        counter += 1;
        let step_id = executor.new_step(
            Action::Volume(VolumeAction {
                name: volume.name.to_string(),
                driver: volume.driver.clone(),
                resolved,
            }),
            BTreeSet::new(),
        );
        if let Some((_, _, old_span)) =
            volume_name_to_dependency.insert(volume.name.to_string(), (resolved, step_id, volume.name.span().clone()))
        {
            DuplicateName::from_spans(&old_span, volume.name.span())?
        }
    }
    let volume_name_to_dependency = volume_name_to_dependency
        .into_iter()
        .map(|(name, (reference, step, _))| (name, (reference, step)))
        .collect::<HashMap<_, _>>();

    logger.log("Queueing secrets");
    let mut counter = 1;
    let mut secret_to_dependency = HashMap::new();
    for container in &document.containers {
        for secret in &container.secrets {
            if !secret_to_dependency.contains_key(secret.name.as_str()) {
                let resolved = ResolvedSecretRef(counter);
                counter += 1;
                let step_id = executor.new_step(
                    Action::Secret(SecretAction {
                        resolved,
                        name: secret.name.clone(),
                    }),
                    BTreeSet::new(),
                );
                secret_to_dependency.insert(secret.name.to_string(), (resolved, step_id));
            }
        }
    }

    logger.log("Queueing containers");
    let mut existing_names = HashMap::new();
    for container in document.containers {
        if let Some(existing) = existing_names.insert(container.name.to_string(), container.name.span().clone()) {
            DuplicateName::from_spans(&existing, container.name.span())?
        }

        logger.trace("Checking injects");

        {
            let mut map = HashMap::with_capacity(container.injects.len());

            for inject in container.injects.iter() {
                if let Some(other) = map.insert(inject.at.as_os_str(), inject.at.span().clone()) {
                    Err(DuplicateInjectPath {
                        content: NamedSource::new(other.file.to_string_lossy(), std::fs::read_to_string(&other.file).d()?),
                        first: other.source_span(),
                        second: inject.at.span().source_span(),
                    })?
                }
            }
        }

        let (image_reference, image_step) = match image_name_to_dependency.get(container.image.as_str()) {
            Some(v) => v,
            None => return UnknownThing::build(container.image, "image"),
        };

        let mut dependencies = vec![*image_step];

        let mut networks = Vec::new();
        for network in container.networks {
            let (reference, step) = match network_name_to_dependency.get(network.name.as_str()) {
                Some(v) => v,
                None => return UnknownThing::build(network.name, "network"),
            };
            networks.push(ContainerActionNetwork {
                resolved: *reference,
                aliases: network.aliases,
            });
            dependencies.push(*step);
        }

        let mut volumes = Vec::new();
        let mut binds = Vec::new();
        for mount in container.mounts {
            match mount {
                ParsedContainerMount::Volume(volume) => {
                    let (reference, step) = match volume_name_to_dependency.get(volume.name.as_str()) {
                        Some(v) => v,
                        None => return UnknownThing::build(volume.name, "volume"),
                    };
                    volumes.push(ContainerActionVolumeMount {
                        name_ref: *reference,
                        destination: volume.destination,
                    });
                    dependencies.push(*step);
                }
                ParsedContainerMount::Bind(bind) => binds.push(ContainerActionBindMount {
                    source: bind.source,
                    destination: bind.destination,
                }),
            }
        }

        let mut secrets = Vec::new();
        for secret in container.secrets {
            let (reference, step) = match secret_to_dependency.get(secret.name.as_str()) {
                Some(v) => v,
                None => return UnknownThing::build(secret.name, "secret"),
            };
            secrets.push(ContainerActionSecret {
                name_ref: *reference,
                target: secret.target.unwrap_or_else(|| secret.name.to_string()),
            });
            dependencies.push(*step);
        }

        let command = if let Some(command) = container.command {
            match shlex::split(&command) {
                Some(command) => Some(command),
                None => Err(MalformedCommand {
                    content: read_source(command.span())?,
                    here: command.span().source_span(),
                })?,
            }
        } else {
            None
        };

        executor.new_step(
            Action::Container(ContainerAction {
                name: container.name.to_string(),
                command,
                image: *image_reference,
                ports: container
                    .ports
                    .into_iter()
                    .map(|port| match port {
                        ParsedContainerPort::Shorthand(port) => ContainerActionPort {
                            container: port,
                            host: port,
                            protocol: ParsedProtocol::Tcp,
                        },
                        ParsedContainerPort::Explicit(ParsedExplicitContainerPort {
                            container,
                            host,
                            protocol,
                        }) => ContainerActionPort {
                            container,
                            host,
                            protocol,
                        },
                    })
                    .collect(),
                injects: container.injects,
                networks,
                volumes,
                secrets,
                binds,
            }),
            BTreeSet::from_iter(dependencies),
        );
    }

    Ok(())
}
