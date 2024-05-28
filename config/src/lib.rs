mod object_store;

use awscreds::Credentials;
use awsregion::Region;
use serde::Deserialize;
use tracing::debug;
use std::{env, path::Path};
use std::path::PathBuf;
use std::fs::read_to_string;
use anyhow::{anyhow, bail, Result};

pub use object_store::*;


#[derive(Debug, Clone, Deserialize)]
pub struct Server {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone, Deserialize)]
pub struct S3 {
    pub bucket: String,
    pub region: Region,
    pub credentials: Credentials
}

#[derive(Debug, Clone, Deserialize)]
pub struct Export {
    pub prefix: Option<PathBuf>,
    pub filesystem_path: Option<PathBuf>,
    pub filesystem: bool,
    pub s3: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Import {
    pub include: Option<Vec<PathBuf>>,
    #[serde(default)]
    pub exclude: Vec<PathBuf>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Resize {
    pub original: Option<u32>,
    pub xl: Option<u32>,
    pub lg: Option<u32>,
    pub md: Option<u32>,
    pub sm: Option<u32>,
    pub xs: Option<u32>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum TransformVariant {
    None,
    Original,
    Xl,
    Lg,
    #[default]
    Md,
    Sm,
    Xs,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(skip)]
    pub dir: PathBuf,
    pub parallel_img_max: Option<usize>,
    #[serde(default)]
    pub resize: Resize,
    #[serde(default)]
    pub transform_variant: TransformVariant,
    pub import: Option<Import>,
    pub export: Option<Export>,
    pub server: Option<Server>,
    pub s3: Option<S3>,
}

impl Config {
    pub fn new() -> Result<Self> {
        let path = match env::args().nth(1) {
            Some(p) => PathBuf::from(p),
            None => PathBuf::from("."),
        };

        // Allow config path to point to a 'config.toml' file or a dir where it's present.
        let dir = match &path {
            p if p.is_file() => p.parent().unwrap_or_else(|| Path::new(".")).to_owned(),
            p if p.is_dir() => path,
            _ => bail!("Unable to determine config.toml path from {:?}", &path),
        };
        
        let mut file = dir.to_owned();
        file.push("config.toml");

        let contents = read_to_string(file).map_err(|e|
            anyhow!("Unable to read config.toml file to string: {}", e)
        )?;

        let mut config = toml::from_str::<Self>(&contents).map_err(|e|
            anyhow!("Unable to read config.toml file path as toml: {}", e)
        )?;

        debug!("Config dir set to {}", dir.display());

        // Add decided dir path to config
        config.dir = dir;

        Ok(config)
    }
    pub fn import(&self) -> Result<Import> {
        self.import.clone().ok_or_else(|| anyhow!("Import config not defined"))
    }
    pub fn export(&self) -> Result<Export> {
        self.export.clone().ok_or_else(|| anyhow!("Export config not defined"))
    }
    pub fn server(&self) -> Result<Server> {
        self.server.clone().ok_or_else(|| anyhow!("Server config not defined"))
    }
    pub fn s3_mut(&mut self) -> Result<&mut S3> {
        self.s3.as_mut().ok_or_else(|| anyhow!("S3 config not defined"))
    }
}

// pub fn add(left: usize, right: usize) -> usize {
//     left + right
// }

// #[cfg(test)]
// mod tests {
//     use super::*;

//     #[test]
//     fn it_works() {
//         let result = add(2, 2);
//         assert_eq!(result, 4);
//     }
// }
