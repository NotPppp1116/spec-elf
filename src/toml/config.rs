use anyhow::{Context, Result};
use serde::Deserialize;
use std::{
    env::{self, current_dir},
    fs::OpenOptions,
    io::Read,
    path::Path,
};

use crate::{
    builder::compile::{Levels, compile_c,compile_lang, compile_rust}, toml::config::{Languages::Rust, Options::V3},
};

// the objective is to create a toml file that allows config
//opening new possibilities
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Languages {
    C,
    Cpp,
    Zig,
    Rust,
}
#[derive(Debug, Clone, Deserialize)]
enum Options {
    V1,
    V2,
    V3,
    V4,
    Native,
}

#[derive(Debug, Clone, Deserialize)]
struct Config {
    language: Languages,
    targets: Option<Vec<Options>>,
    output:Option<String>,
}

trait Loading: Sized {
    fn load(project_dir: &Path) -> Result<Option<Self>>;
}
const NAME: &'static str = "spec-elf.toml";
impl Loading for Config {
    //loads the config
    fn load(project_dir: &Path) -> Result<Option<Self>> {
        let config_path = project_dir.join(NAME);

        if !config_path.is_file() {
            return Ok(None);
        }

        let mut file = OpenOptions::new()
            .read(true)
            .open(&config_path)
            .with_context(|| format!("failed to open {}", config_path.display()))?;

        let mut contents = String::new();
        file.read_to_string(&mut contents)
            .with_context(|| format!("failed to read {}", config_path.display()))?;

        //loads the config to return
        let config: Config = toml::from_str(&contents)
            .with_context(|| format!("failed to parse {}", config_path.display()))?;

        Ok(Some(config))
    }
}
pub fn check_config() -> Result<bool> {
    let dir = env::current_dir()?;
    Ok(dir.join(NAME).exists())
}

impl Config {
    //resolve the targets if empty use the all default
    fn targets_resolve(&self) -> Vec<Options> {
        self.targets.clone().unwrap_or_else(|| {
            vec![
                Options::V1,
                Options::V2,
                Options::V3,
                Options::V4,
                Options::Native,
            ]
        })
    }
    fn build_flags(&self) -> Levels {
        let targest = self.targets_resolve();

        //build flags  parameter
        let mut levels = Levels::empty();
        for item in targest {
            match item {
                Options::V1 => levels.insert(Levels::V1),
                Options::V2 => levels.insert(Levels::V2),
                Options::V3 => levels.insert(Levels::V3),
                Options::V4 => levels.insert(Levels::V4),
                Options::Native => levels.insert(Levels::NATIVE),
            }
        }
        levels
    }
 
}
