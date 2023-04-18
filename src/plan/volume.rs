use podman_api::{
    models::Volume,
    opts::{VolumeCreateOpts, VolumeListFilter, VolumeListOpts},
    Podman,
};

use super::{PostAction, StepContext};
use crate::utils::{IntoDiagnosticShorthand, XTug};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct ResolvedVolumeRef(pub usize);

#[derive(Clone, Debug)]
pub struct VolumeAction {
    pub name: String,
    pub driver: String,
    pub resolved: ResolvedVolumeRef,
}

pub async fn execute(ctx: &StepContext, action: VolumeAction) -> miette::Result<()> {
    let remote_volumes = remote_volume_query(&ctx.service, ctx.group.clone(), action.name.clone()).await?;

    if remote_volumes.is_empty() {
        return create_volume(ctx, action).await;
    }

    let first_volume = remote_volumes.first().unwrap();

    if remote_volumes.len() == 1 && first_volume.driver == action.driver {
        ctx.resolved_volumes.lock().insert(action.resolved, first_volume.name.clone());
        return Ok(());
    }

    for volume in remote_volumes {
        ctx.finalize.lock().push(PostAction::DeleteVolume { name: volume.name });
    }

    create_volume(ctx, action).await
}

pub async fn remote_volume_query(service: &Podman, group: String, name: String) -> miette::Result<Vec<Volume>> {
    service
        .volumes()
        .list(
            &VolumeListOpts::builder()
                .filter([
                    VolumeListFilter::LabelKeyVal(XTug::Group.to_string(), group.clone()),
                    VolumeListFilter::LabelKeyVal(XTug::Name.to_string(), name),
                ])
                .build(),
        )
        .await
        .d()
}

async fn create_volume(ctx: &StepContext, action: VolumeAction) -> miette::Result<()> {
    let volume = ctx
        .service
        .volumes()
        .create(
            &VolumeCreateOpts::builder()
                .driver(action.driver)
                .labels([(XTug::Group, &ctx.group), (XTug::Name, &action.name)])
                .build(),
        )
        .await
        .d()?;

    let name = volume.name.unwrap();
    ctx.resolved_volumes.lock().insert(action.resolved, name.clone());
    ctx.backtrack.lock().push(PostAction::DeleteVolume { name });

    Ok(())
}
