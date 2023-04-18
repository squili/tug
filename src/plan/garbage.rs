use podman_api::opts::{ContainerListFilter, ContainerListOpts};

use super::{PostAction, StepContext};
use crate::utils::{IntoDiagnosticShorthand, XTug};

#[derive(Clone, Debug)]
pub struct GarbageAction {
    pub container_names: Vec<String>,
}

pub async fn execute(ctx: &StepContext, action: GarbageAction) -> miette::Result<()> {
    let remote_containers = ctx
        .service
        .containers()
        .list(
            &ContainerListOpts::builder()
                .all(true)
                .filter([
                    ContainerListFilter::LabelKeyVal(XTug::Group.to_string(), ctx.group.clone()),
                    ContainerListFilter::LabelKey(XTug::Name.to_string()),
                ])
                .build(),
        )
        .await
        .d()?;

    let mut to_stop = Vec::new();

    for container in remote_containers {
        if let (Some(id), Some(name)) = (container.id, container.labels.unwrap_or_default().remove(XTug::Name.as_ref())) {
            if !action.container_names.contains(&name) {
                if container.status.as_deref() == Some("running") {
                    to_stop.push(id.clone());
                    ctx.backtrack.lock().push(PostAction::RestartContainer { id: id.clone() });
                }
                ctx.finalize.lock().push(PostAction::DeleteContainer { id });
            }
        }
    }

    futures_util::future::try_join_all(to_stop.into_iter().map(|id| {
        tokio::spawn({
            let service = ctx.service.clone();
            let container = service.containers().get(id);
            async move { container.stop(&Default::default()).await }
        })
    }))
    .await
    .d()?;

    Ok(())
}
