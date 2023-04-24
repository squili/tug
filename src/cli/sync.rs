use std::path::PathBuf;

use clap::Parser;

use crate::{config::Config, logger::Logger, plan::Executor};

#[derive(Parser)]
pub struct Args {
    directory: PathBuf,
}

impl Args {
    pub async fn execute(self, config: Config, logger: Logger) -> miette::Result<()> {
        let document = crate::parse::parse(&logger, &self.directory)?;
        let mut executor = Executor::new();
        crate::prepare::prepare(&logger, document, &mut executor)?;
        let service = config.service(&logger, false).await?;
        logger.log("Executing plan");
        executor.execute(&config, &logger, service, &self.directory).await?;
        logger.log("Done!");

        Ok(())
    }
}
