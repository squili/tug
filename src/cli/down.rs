use clap::Parser;
use podman_api::opts::{ContainerListFilter, ContainerListOpts, NetworkListFilter, NetworkListOpts};

use crate::{
    config::Config,
    logger::Logger,
    utils::{IntoDiagnosticShorthand, XTug},
};

#[derive(Parser)]
pub struct Args {}

impl Args {
    pub async fn execute(self, config: Config, logger: Logger) -> miette::Result<()> {
        let service = config.service(&logger, false).await?;
        logger.log("Containers");
        let containers = service
            .containers()
            .list(
                &ContainerListOpts::builder()
                    .all(true)
                    .filter([ContainerListFilter::LabelKeyVal(
                        XTug::Group.to_string(),
                        config.group.clone(),
                    )])
                    .build(),
            )
            .await
            .d()?;
        for container in containers {
            let running = container.state.as_deref() == Some("running");
            let container = service.containers().get(container.id.unwrap());
            logger.log(container.id());
            if running {
                container.stop(&Default::default()).await.d()?;
            }
            container.delete(&Default::default()).await.d()?;
        }
        logger.log("Networks");
        let networks = service
            .networks()
            .list(
                &NetworkListOpts::builder()
                    .filter([NetworkListFilter::LabelKeyVal(XTug::Group.to_string(), config.group.clone())])
                    .build(),
            )
            .await
            .d()?;
        for network in networks {
            let id = network.id.unwrap();
            logger.log(&id);
            let network = service.networks().get(id);
            network.delete().await.d()?;
        }

        Ok(())
    }
}
