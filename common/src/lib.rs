use serde::Deserialize;
use std::{env, path::Path};
use std::path::PathBuf;
use std::fs::read_to_string;
use anyhow::{Result, bail};

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConf {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone, Deserialize)]
pub struct S3Conf {
    pub host: String,
    pub port: u16,
    pub region: String,
    pub bucket_name: String,
    pub access_key: Option<String>,
    pub secret_key: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub prefix: Option<PathBuf>,
    pub dirs: Vec<PathBuf>,
    pub server: ServerConf,
    pub s3: S3Conf,
}

impl Config {
    pub fn paths_from_args() -> Result<(PathBuf, PathBuf)> {
        let mut dir = PathBuf::from(".");
        let mut config = PathBuf::new();

        for (i, a) in env::args().enumerate() {
            if a.is_empty() {
                continue;
            }

            match i {
                1 => {
                    let arg_dir = PathBuf::from(a);

                    match arg_dir.is_relative() {
                        true => dir.push(arg_dir),
                        false => {
                            dir = arg_dir;
                        }
                    }

                    if ! dir.is_dir() {
                        bail!("Given working dir (from arg 1) doesn't exist")
                    }
                },
                2 => {
                    let arg_conf = PathBuf::from(a);

                    match arg_conf.is_file() {
                        true => {
                            config = arg_conf;
                        },
                        false => {
                            config = dir.to_owned();
                            config.push(arg_conf);
                        }
                    }
                }
                _ => (),
            }
        }

        // Config file not defined, presume it's in working dir.
        if config.eq(&PathBuf::new()) {
            config = dir.to_owned();
            config.push("config.toml");
        }

        if ! config.is_file() {
            bail!("Unable to find config file from {:?}", config)
        }

        Ok((dir, config))
    }
    pub fn from_path<T>(path: T, prefix: Option<&PathBuf>) -> Result<Self>
    where T: AsRef<Path> {
        let s = match read_to_string(path) {
            Ok(s) => s,
            Err(e) => panic!("Unable to read config file to string: {}", e),
        };

        let mut conf = match toml::from_str::<Self>(&s) {
            Ok(c) => c,
            Err(e) => panic!("Unable to read config file path as toml: {}", e),
        };

        if conf.prefix.is_none() && prefix.is_some() {
            conf.prefix = prefix.map(|d|d.to_owned());
        }

        Ok(conf)
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
