use clap::Parser;

use crate::{config::Config, logger::Logger};

#[derive(Parser)]
pub struct Args {
    name: String,
}

impl Args {
    pub async fn execute(self, config: Config, logger: Logger) -> miette::Result<()> {
        let service = config.service(&logger, true).await?;
        let networks = crate::plan::network::remote_network_query(&service, config.group, self.name).await?;
        for network in networks {
            println!("{}", network.id.unwrap());
        }

        Ok(())
    }
}
