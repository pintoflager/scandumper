use std::env;
use std::path::PathBuf;
use anyhow::{Result, anyhow, bail};
use s3::creds::Credentials;
use s3::error::S3Error;
use s3::{Bucket, BucketConfiguration, Region, Tag};
use tracing::debug;

use common::Config;

#[derive(Clone, Debug)]
pub struct ObjectStore {
    name: String,
    region: Region,
    credentials: Credentials
}

impl ObjectStore {
    pub fn init(config: &Config) -> Result<Self> {
        let name = config.s3.bucket_name.to_owned();

        let region = Region::Custom {
            region: config.s3.region.to_owned(),
            endpoint: format!("http://{}:{}", config.s3.host, config.s3.port),
        };
    
        let credentials = Credentials::new(
            config.s3.access_key.to_owned().or(
                env::var("S3_ACCESS_KEY").ok()
            ).as_deref(),
            config.s3.secret_key.to_owned().or(
                env::var("S3_SECRET_KEY").ok()
            ).as_deref(),
            None, None, None
        )
        .map_err(|e| anyhow!("Failed to init S3 credentials: {}", e))?;

        Ok(Self { name, region, credentials })
    }
}

#[derive(Clone, Debug)]
pub struct Objects {
    pub obj_store: ObjectStore,
    pub bucket: Bucket,
    verified: bool 
}

impl Objects {
    pub fn from(object_store: &ObjectStore) -> Result<Self> {
        let bucket = Bucket::new(&object_store.name, object_store.region.clone(), object_store.credentials.clone())
            .map_err(|e| anyhow!("Failed to create S3 bucket: {}", e))?
            .with_path_style();

        Ok(Self { obj_store: object_store.to_owned(), bucket, verified: false })
    }
    pub async fn verify_existence(&mut self, create: bool) -> Result<()> {
        // Poke object store to see if bucket exists
        if let Err(e) = self.bucket.list("/".to_string(), None).await {
            match e {
                // This is what a non existing bucket is expected to respond.
                S3Error::Http(c, s) => match c {
                    404 => match create {
                        true => {
                            debug!("Bucket doesn't exist yet: {} / {}", c, s);

                            self.bucket = Bucket::create_with_path_style(
                                &self.obj_store.name,
                                self.obj_store.region.to_owned(),
                                self.obj_store.credentials.to_owned(),
                                BucketConfiguration::default(),
                            )
                            .await
                            .map_err(|e| anyhow!("Could not create bucket: {}", e))?
                            .bucket;

                            self.verified = true;
                        },
                        false => bail!("Bucket doens't exist and create was not allowed")
                    }
                    _ => bail!("Bucket of shit. Http error: {} / {}", c, s),
                },
                _ => bail!("Bucket of shit: {}", e),
            }
        }

        self.verified = true;

        Ok(())
    }
    pub async fn get_tags<P>(&self, key: P) -> Result<Vec<Tag>> where P: AsRef<str> {
        match self.bucket.get_object_tagging(&key).await {
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
    pub async fn tag_source(&self, sha: &String, key: &PathBuf) -> Result<String> {
        let path = key.to_string_lossy();
    
        let response = self.bucket.put_object_tagging(path.as_ref(), &[
            ("sha256", sha)
        ]).await;
    
        match response {
            Ok(_) => Ok(format!("{}: S3 object SHAsum tag add OK", path.as_ref())),
            Err(e) => bail!("{}: S3 object tag add failed: {}", path.as_ref(), e),
        }
    }
}