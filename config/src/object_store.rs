use std::env;
use std::path::PathBuf;
use anyhow::{Result, anyhow, bail};
use s3::error::S3Error;
use s3::{Bucket, BucketConfiguration, Tag};
use tracing::debug;

use super::S3 as S3Config;


#[derive(Clone, Debug)]
pub struct ObjectStore(pub Bucket);

impl ObjectStore {
    pub async fn init_from(config: &mut S3Config, create: bool) -> Result<()> {
        if config.credentials.access_key.is_none() {
            config.credentials.access_key = env::var("S3_ACCESS_KEY").ok();
        }

        if config.credentials.secret_key.is_none() {
            config.credentials.secret_key = env::var("S3_SECRET_KEY").ok();
        }

        let mut store = Self::get(&config)?;

        store.exists(&config.bucket, &config, create).await
    }
    pub fn get(config: &S3Config) -> Result<Self> {
        let bucket = Bucket::new(&config.bucket, config.region.clone(), config.credentials.clone())
            .map_err(|e| anyhow!("Failed to create S3 bucket: {}", e))?
            .with_path_style();

        let store = Self(bucket);

        Ok(store)
    }
    // pub fn new<T>(name: T, config: &S3Config) -> Result<Self>
    // where T: AsRef<str> {
    //     let bucket = Bucket::new(name.as_ref(), config.region.clone(), config.credentials.clone())
    //         .map_err(|e| anyhow!("Failed to create S3 bucket: {}", e))?
    //         .with_path_style();

    //     let store = Self(bucket);

    //     Ok(store)
    // }
    async fn exists<T>(&mut self, name: T, config: &S3Config, create: bool) -> Result<()>
    where T: AsRef<str> {
        // Poke object store to see if bucket exists
        if let Err(e) = self.0.list("/".to_string(), None).await {
            match e {
                // This is what a non existing bucket is expected to respond.
                S3Error::Http(c, s) => match c {
                    404 => match create {
                        true => {
                            debug!("Bucket did not exist, creating: {} / {}", c, s);

                            self.0 = Bucket::create_with_path_style(
                                name.as_ref(),
                                config.region.to_owned(),
                                config.credentials.to_owned(),
                                BucketConfiguration::default(),
                            )
                            .await
                            .map_err(|e| anyhow!("Could not create bucket: {}", e))?
                            .bucket;
                        },
                        false => bail!("Bucket does not exist and create was not requested")
                    }
                    _ => bail!("Shitbucket had unexpected http error: {} / {}", c, s),
                },
                _ => bail!("Shitbucket: {}", e),
            }
        }

        Ok(())
    }
    pub async fn get_tags<P>(&self, key: P) -> Result<Vec<Tag>> where P: AsRef<str> {
        match self.0.get_object_tagging(&key).await {
            Ok((v, _)) => return Ok(v),
            Err(e) => match e {
                S3Error::Http(c, s) => match c {
                    404 => return Ok(vec![]),
                    _ => bail!("Get tags http error: {} ({})", s, c),
                },
                _ => bail!("Get tags error: {}", e),
            }
        }
    }
    pub async fn tag_source(&self, checksum: &String, key: &PathBuf) -> Result<()> {
        let path = key.to_string_lossy();
    
        let response = self.0.put_object_tagging(path.as_ref(), &[
            ("checksum", checksum)
        ]).await;
    
        match response {
            Ok(_) => Ok(()),
            Err(e) => bail!("{}: S3 object tag add failed: {}", path.as_ref(), e),
        }
    }
}