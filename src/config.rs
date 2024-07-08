use std::path::{Path, PathBuf};

use figment::{
    providers::{Env, Format, Serialized, Toml},
    Figment,
};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone)]
pub struct Project {
    pub token: String,
    pub location: String,
    #[serde(default = "compose_file_default")]
    pub compose_file: String,
    pub package_names: Option<Vec<String>>,
}

fn compose_file_default() -> String {
    "docker-compose.yaml".into()
}

impl Project {
    pub fn compose_path(&self) -> PathBuf {
        let directory = Path::new(&self.location);
        directory.join(&self.compose_file)
    }
}

#[derive(Deserialize, Serialize, Clone)]
pub struct Config {
    pub host: String,
    pub port: u16,
    pub projects: Vec<Project>,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            host: "localhost".into(),
            port: 5000,
            projects: Vec::new(),
        }
    }
}

pub fn parse_config() -> Result<Config, figment::Error> {
    Figment::from(Serialized::defaults(Config::default()))
        .merge(Toml::file("App.toml"))
        .merge(Env::prefixed("APP_"))
        .extract()
}
