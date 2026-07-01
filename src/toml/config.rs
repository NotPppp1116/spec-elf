use anyhow::{Context, Result};
use serde::Deserialize;
use std::{fs::OpenOptions, io::Read, path::Path};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Languages {
    C,
    Cpp,
    Zig,
    Rust,
}

#[derive(Debug, Clone, Deserialize)]
struct Config {
    language: Option<Languages>,
    command: Option<String>,
    artifact: Option<String>,
}

impl Config {
    fn load(project_dir: &Path) -> Result<Option<Self>> {
        let config_path = project_dir.join("spec-elf.toml");

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

        let config: Config = toml::from_str(&contents)
            .with_context(|| format!("failed to parse {}", config_path.display()))?;

        Ok(Some(config))
    }
    
}
