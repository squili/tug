use podman_api::{
    models::Network,
    opts::{NetworkCreateOpts, NetworkListFilter, NetworkListOpts},
    Podman,
};

use super::{PostAction, StepContext};
use crate::utils::{IntoDiagnosticShorthand, XTug};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct ResolvedNetworkRef(pub usize);

#[derive(Clone, Debug)]
pub struct NetworkAction {
    pub name: String,
    pub dns_enabled: bool,
    pub internal: bool,
    pub driver: String,
    pub resolved: ResolvedNetworkRef,
}

pub async fn execute(ctx: &StepContext, action: NetworkAction) -> miette::Result<()> {
    let remote_networks = remote_network_query(&ctx.service, ctx.group.clone(), action.name.clone()).await?;

    if remote_networks.is_empty() {
        return create_network(ctx, action).await;
    }

    let first_network = remote_networks.first().unwrap();

    if remote_networks.len() == 1
        && first_network.dns_enabled == Some(action.dns_enabled)
        && first_network.driver.as_ref() == Some(&action.driver)
        && first_network.internal == Some(action.internal)
    {
        ctx.resolved_networks
            .lock()
            .insert(action.resolved, first_network.name.as_ref().unwrap().clone());
        return Ok(());
    }

    for network in remote_networks {
        ctx.finalize
            .lock()
            .push(PostAction::DeleteNetwork { id: network.id.unwrap() });
    }

    create_network(ctx, action).await
}

async fn create_network(ctx: &StepContext, action: NetworkAction) -> miette::Result<()> {
    let network = ctx
        .service
        .networks()
        .create(
            &NetworkCreateOpts::builder()
                .dns_enabled(action.dns_enabled)
                .driver(action.driver)
                .internal(action.internal)
                .labels([(XTug::Group, ctx.group.clone()), (XTug::Name, action.name.clone())])
                .build(),
        )
        .await
        .d()?;

    ctx.backtrack
        .lock()
        .push(PostAction::DeleteNetwork { id: network.id.unwrap() });
    ctx.resolved_networks.lock().insert(action.resolved, network.name.unwrap());

    Ok(())
}

pub async fn remote_network_query(service: &Podman, group: String, name: String) -> miette::Result<Vec<Network>> {
    service
        .networks()
        .list(
            &NetworkListOpts::builder()
                .filter([
                    NetworkListFilter::LabelKeyVal(XTug::Group.to_string(), group.clone()),
                    NetworkListFilter::LabelKeyVal(XTug::Name.to_string(), name),
                ])
                .build(),
        )
        .await
        .d()
}
