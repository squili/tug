use std::path::PathBuf;

use clap::Parser;

use crate::logger::Logger;

#[derive(Parser)]
pub struct Args {
    root: PathBuf,
}

impl Args {
    pub async fn execute(self, logger: Logger) -> miette::Result<()> {
        let doc = crate::parse::parse(&logger, &self.root)?;
        println!("{doc:?}");

        Ok(())
    }
}
