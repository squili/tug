use std::path::{Path, PathBuf};

use figment::{
    providers::{Env, Format, Toml},
    Figment,
};
use podman_api::Podman;
use serde::Deserialize;

use crate::{logger::Logger, utils::IntoDiagnosticShorthand};

#[derive(Deserialize, Debug)]
pub struct Config {
    service: String,
    #[serde(default = "default_group")]
    pub group: String,
}

fn default_group() -> String {
    "default".into()
}

pub fn config_file() -> PathBuf {
    match std::env::var("TUG_CONFIG") {
        Ok(config_file) => Path::new(&config_file).to_path_buf(),
        Err(_) => dirs::config_dir().expect("config file should exist").join("tug.toml"),
    }
}

pub fn load() -> Result<Config, figment::Error> {
    Figment::new()
        .join(Env::prefixed("TUG_"))
        .join(Toml::file(config_file()))
        .extract()
}

impl Config {
    pub async fn service(&self, logger: &Logger, silent: bool) -> miette::Result<Podman> {
        if !silent {
            logger.log("Connecting to container runtime");
        }
        let service = Podman::new(&self.service).d()?;
        Ok(service)
    }
}
