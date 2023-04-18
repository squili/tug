use clap::Parser;

use crate::{config::Config, logger::Logger, utils::IntoDiagnosticShorthand};

#[derive(Parser)]
pub struct Args {}

impl Args {
    pub async fn execute(self, config: Config, logger: Logger) -> miette::Result<()> {
        let service = config.service(&logger, false).await?;
        logger.log("Ping...");
        service.info().await.d()?;
        logger.log("...Pong!");
        Ok(())
    }
}
