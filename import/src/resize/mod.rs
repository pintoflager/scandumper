pub mod jpeg;
pub mod png;

use std::path::PathBuf;
use fast_image_resize::images::Image;
use fast_image_resize::{IntoImageView, ResizeOptions};
use fast_image_resize::Resizer;
use anyhow::{Result, bail};
use tokio::fs::create_dir_all;
use tokio::task::JoinSet;
use std::future::Future;
use tracing::debug;

use config::{Config, ObjectStore, TransformVariant};

use crate::transform::Transformable;


#[derive(Debug, Clone)]
pub enum TargetSize {
    Original(u32),
    Xl(u32),
    Lg(u32),
    Md(u32),
    Sm(u32),
    Xs(u32),
}

impl TargetSize {
    pub fn original(config: &Config) -> Self {
        match config.resize.original {
            Some(u) => TargetSize::Original(u),
            None => TargetSize::Original(2500),
        }
    }
    pub fn xl(config: &Config) -> Self {
        match config.resize.xl {
            Some(u) => TargetSize::Xl(u),
            None => TargetSize::Xl(1200),
        }
    }
    pub fn lg(config: &Config) -> Self {
        match config.resize.lg {
            Some(u) => TargetSize::Lg(u),
            None => TargetSize::Lg(600),
        }
    }
    pub fn md(config: &Config) -> Self {
        match config.resize.md {
            Some(u) => TargetSize::Md(u),
            None => TargetSize::Md(300),
        }
    }
    pub fn sm(config: &Config) -> Self {
        match config.resize.sm {
            Some(u) => TargetSize::Sm(u),
            None => TargetSize::Sm(150),
        }
    }
    pub fn xs(config: &Config) -> Self {
        match config.resize.xs {
            Some(u) => TargetSize::Xs(u),
            None => TargetSize::Xs(75),
        }
    }
    pub fn to_str(&self) -> &'static str {
        match self {
            TargetSize::Original(_) => "og",
            TargetSize::Xl(_) => "xl",
            TargetSize::Lg(_) => "lg",
            TargetSize::Md(_) => "md",
            TargetSize::Sm(_) => "sm",
            TargetSize::Xs(_) => "xs",
        }
    }
    pub fn to_px(&self) -> u32 {
        match self {
            TargetSize::Original(u) |
            TargetSize::Xl(u) |
            TargetSize::Lg(u) |
            TargetSize::Md(u) |
            TargetSize::Sm(u) |
            TargetSize::Xs(u) => *u,
        }
    }
    pub fn transform_size_variant(config: &Config) -> Option<Self> {
        match config.transform_variant {
            TransformVariant::None => None,
            TransformVariant::Original => Some(TargetSize::original(config)),
            TransformVariant::Xl => Some(TargetSize::xl(config)),
            TransformVariant::Lg => Some(TargetSize::lg(config)),
            TransformVariant::Md => Some(TargetSize::md(config)),
            TransformVariant::Sm => Some(TargetSize::sm(config)),
            TransformVariant::Xs => Some(TargetSize::xs(config)),
        }
    }
}

pub struct ResizeStats {
    pub succeeded: Vec<String>,
    pub skipped: Vec<String>,
    pub failed: Vec<String>,
}

impl ResizeStats {
    pub fn new() -> Self {
        Self { succeeded: vec![], skipped: vec![], failed: vec![] }
    }
    pub fn push(&mut self, response: Result<String>) {
        match response {
            Ok(s) => self.succeeded.push(s),
            Err(e) => self.failed.push(e.to_string()),
        }
    }
    pub fn extend(&mut self, child: Result<Self>) {
        match child {
            Ok(s) => {
                self.succeeded.extend(s.succeeded);
                self.skipped.extend(s.skipped);
                self.failed.extend(s.failed);
            },
            Err(e) => {
                self.failed.push(e.to_string());
            },
        }
    }
}

pub async fn resize_handler<F>(items: &[(u32, &str)], transformable: &Transformable, original: &impl IntoImageView,
options: &Option<ResizeOptions>, s3: &Option<ObjectStore>, fs: &Option<PathBuf>,
writer: impl Fn(Image<'static>) -> F + Send + 'static + Copy) -> Result<ResizeStats>
where
    F: Send + Sized,
    F: Future<Output = Result<Vec<u8>, anyhow::Error>>
{
    let resizables = transformable.get_resizables(items, s3, fs).await;
    let mut stats = ResizeStats::new();
    let mut resizer = Resizer::new();
    
    if resizables.is_empty() {
        stats.skipped.push(format!("Image {:?} already resized", transformable.source_path));

        return Ok(stats)
    }

    let mut handles = JoinSet::new();

    for (ratio, id, resized_file) in resizables {
        let (w, h) = transformable.convert_ratio(ratio);
        let mut resized = Image::new(w, h, transformable.pixel);
        let checksum = transformable.checksum.to_string();

        debug!(
            "Resizing input image {} to path {:?} to width: {} and height: {}...",
            id.to_uppercase(), resized_file, w, h
        );

        // Resize source image into buffer of destination image
        resizer.resize(original, &mut resized, options)?;
    
        // Create new bucket for each task: https://github.com/durch/rust-s3/issues/337
        // v.0.34.0 of rust-s3 should fix this issue
        let s3 = s3.clone();
        let fs = fs.clone();
        let mime = match transformable.target_ext {
            "png" => "image/png",
            "jpeg" => "image/jpeg",
            _ => bail!("Unsupported image format: {:?}", transformable.target_ext),
        };

        // Create target dir if filesystem is selected
        let resized_dir = match resized_file.parent() {
            Some(p) => p.to_owned(),
            None => bail!("Failed to extract parent dir from resized image path {:?}", resized_file),
        
        };

        // Create resized image dir if filesystem exporting is selected
        if fs.is_some() {
            if let Err(e) = create_dir_all(&resized_dir).await {
                panic!("Failed to create target dir for resized images {}: {}", resized_dir.display(), e)
            }

            debug!("Created target dir for resized images: {:?}", &resized_dir);
        }

        handles.spawn(async move {
            let mut targets = vec![];

            // Read resized image into bytes for writing
            let buf = match writer(resized).await {
                Ok(v) => v,
                Err(e) => {
                    bail!("{}: Failed to read image to bytes: {}", resized_file.display(), e)
                }
            
            };
            
            // Write image into the filesystem
            if fs.is_some() {
                targets.push("filesystem");

                // Write resized image to file
                if let Err(e) = tokio::fs::write(&resized_file, &buf).await {
                    bail!("Failed to write resized image {}: {}", resized_file.display(), e)
                }

                // Write checksum to file next to the resized image
                let mut checksum_file = resized_dir;
                checksum_file.push(format!(".{}.checksum", id));

                if let Err(e) = tokio::fs::write(&checksum_file, &checksum).await {
                    bail!("Failed to write checksum file {}: {}", checksum_file.display(), e)
                }
            }
            
            // Write image into the S3 bucket with checksum tag
            if let Some(s) = s3 {
                targets.push("S3");

                let s3_path = resized_file.to_string_lossy();

                if let Err(e) = s.0.put_object_with_content_type(s3_path.as_ref(), &buf, mime).await {
                    bail!("{}: Failed to store S3 object: {}", s3_path.as_ref(), e)
                }

                s.tag_source(&checksum, &resized_file).await?;
            }

            Ok(format!("Resized image {} / {} saved successfully to {}", resized_file.display(), id, targets.join(", ")))
        });
    }

    while let Some(r) = handles.join_next().await {
        let response = r.expect("Failed to execute spawned task");
        stats.push(response);
    }

    Ok(stats)
}
