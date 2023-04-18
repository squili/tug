mod debug;
mod down;
mod query;
mod sync;

use clap::Parser;

use crate::{config::Config, logger::Logger};

#[derive(Parser)]
pub struct Args {
    #[clap(subcommand)]
    subcommand: Subcommand,
}

#[derive(Parser)]
pub enum Subcommand {
    Debug(debug::Args),
    Down(down::Args),
    Query(query::Args),
    Sync(sync::Args),
}

impl Args {
    pub async fn execute(self, config: Config, logger: Logger) -> miette::Result<()> {
        match self.subcommand {
            Subcommand::Debug(args) => args.execute(config, logger).await,
            Subcommand::Down(args) => args.execute(config, logger).await,
            Subcommand::Query(args) => args.execute(config, logger).await,
            Subcommand::Sync(args) => args.execute(config, logger).await,
        }
    }
}
