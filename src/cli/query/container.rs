use clap::Parser;

use crate::{config::Config, logger::Logger};

#[derive(Parser)]
pub struct Args {
    name: String,
}

impl Args {
    pub async fn execute(self, config: Config, logger: Logger) -> miette::Result<()> {
        let service = config.service(&logger, true).await?;
        let containers = crate::plan::container::remote_containers_query(&service, config.group, self.name).await?;
        for container in containers {
            println!("{}", container.id.unwrap());
        }

        Ok(())
    }
}
