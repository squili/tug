mod container;
mod network;
pub mod volume;

use clap::Parser;

use crate::{config::Config, logger::Logger};

#[derive(Parser)]
pub struct Args {
    #[clap(subcommand)]
    subcommand: Subcommand,
}

#[derive(Parser)]
pub enum Subcommand {
    Container(container::Args),
    Network(network::Args),
    Volume(volume::Args),
}

impl Args {
    pub async fn execute(self, config: Config, logger: Logger) -> miette::Result<()> {
        match self.subcommand {
            Subcommand::Container(args) => args.execute(config, logger).await,
            Subcommand::Network(args) => args.execute(config, logger).await,
            Subcommand::Volume(args) => args.execute(config, logger).await,
        }
    }
}
