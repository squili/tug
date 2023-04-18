mod ping;
mod validate;

use clap::Parser;

use crate::{config::Config, logger::Logger};

#[derive(Parser)]
pub struct Args {
    #[clap(subcommand)]
    subcommand: Subcommand,
}

#[derive(Parser)]
pub enum Subcommand {
    Ping(ping::Args),
    Validate(validate::Args),
}

impl Args {
    pub async fn execute(self, config: Config, logger: Logger) -> miette::Result<()> {
        match self.subcommand {
            Subcommand::Ping(args) => args.execute(config, logger).await,
            Subcommand::Validate(args) => args.execute(logger).await,
        }
    }
}
