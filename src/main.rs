use clap::Parser;
use logger::Logger;
use utils::IntoDiagnosticShorthand;

mod cli;
mod config;
mod logger;
mod parse;
mod plan;
mod prepare;
mod utils;

#[tokio::main(flavor = "current_thread")]
async fn main() -> miette::Result<()> {
    let logger = Logger::new();
    let config = config::load().d()?;
    let args = cli::Args::parse();
    args.execute(config, logger).await
}
