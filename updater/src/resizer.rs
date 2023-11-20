use std::num::NonZeroU32;
use std::path::PathBuf;
use fast_image_resize::{PixelType, Resizer, ResizeAlg, FilterType};
use image::io::Reader as ImageReader;
use image::ImageFormat;
use anyhow::{Result, bail};
use tracing::{debug, info, warn, error};
use crypto::digest::Digest;
use crypto::sha2::Sha256;

use crate::object_store::Objects;

use super::object_store::ObjectStore;
use super::jpeg::*;
use super::png::*;

pub const IMG_TARGET_RATIOS: [(u32, &'static str); 2] = [
    (2500, "lg"),
    (600, "md")
];

pub const THUMBNAIL_TARGET_RATIOS: [(u32, &'static str); 2] = [
    (150, "sm"),
    (30, "xs")
];

#[derive(Clone, Debug)]
pub enum ScaleRef {
    Width(u32),
    Height(u32)
}

#[derive(Clone, Debug)]
pub struct SourceFile {
    pub width: NonZeroU32,
    pub height: NonZeroU32,
    pub scale: ScaleRef,
    pub source_path: PathBuf,
    pub target_path: PathBuf,
    pub pixel: PixelType,
    pub transparency: bool,
    pub sha256sum: String,
}

impl SourceFile {
    pub fn convert_ratio(&self, r: u32) -> (u32, u32) {
        match self.scale {
            ScaleRef::Width(u) => match r > u {
                true => (self.width.get(), self.height.get()),
                false => {
                    let h = r * self.height.get() / u;
                    let w = h * u / self.height.get();
                    (w, h)
                }
            }
            ScaleRef::Height(u) => match r > u {
                true => (self.width.get(), self.height.get()),
                false => {
                    let w = r * self.width.get() / u;
                    let h = w * u / self.width.get();
                    (w, h)
                }
            }
        }
    }
    pub async fn get_resizables(&self, ratios: &[(u32, &str)], s3: &Objects,
    ext: &str) -> Result<Vec<(u32, String, PathBuf)>> {
        let mut resizables = vec![];

        for (r, s) in ratios {
            // Runs checks concurrently to speed up the process.
            let file_path = self.to_target_path(s, ext)?;

            match self.skip_duplicate(&file_path, &s3).await {
                true => { debug!("Path {:?} already exists on S3 bucket", file_path); },
                false => { resizables.push((*r, s.to_string(), file_path)); }
            }
        }

        Ok(resizables)
    }
    async fn skip_duplicate(&self, key: &PathBuf, s3: &Objects) -> bool {
        let path = key.to_string_lossy();

        match s3.get_tags(path).await {
            Ok(v) => match v.is_empty() {
                true => {
                    debug!("Object {:?} does not exist / doesn't have tags", key);
                    false
                },
                false => match v.into_iter().find(|t|t.key().eq("sha256")) {
                    Some(t) => match t.value().eq(&self.sha256sum) {
                        true => {
                            debug!("Object {:?} is same as the provided item, skipping...", key);
                            true
                        },
                        false => {
                            info!("Object {:?} has changed, overwrite...", key);
                            false
                        }
                    },
                    None => {
                        warn!("Object {:?} is missing sha256 tag", key);
                        false
                    },
                }
            },
            Err(e) => {
                error!("Duplicate check failed: {}, weird.", e);
                false
            }
        }
    }
    fn to_target_path(&self, size: &str, ext: &str) -> Result<PathBuf> {
        match self.source_path.file_name() {
            Some(o) => match o.to_str() {
                Some(f) => {
                    let mut path = self.target_path.to_owned();
                    let mut v = f.split('.').collect::<Vec<&str>>();
                    
                    if v.len() > 1 {
                        v.truncate(v.len() - 1);
                    }

                    let f = format!("{}/{}.{}", v.join("."), size, ext);
                    path.push(f);
                    Ok(path)
                },
                None => bail!("Failed to read source image filename to string"),
            },
            None => bail!("Failed to extract filename from source image path")
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

pub async fn img_sizer(path: PathBuf, target: PathBuf, obj_store: ObjectStore) -> Result<ResizeStats> {
    if ! path.is_file() {
        bail!("Dumb ass, provide image file path for resizer: {:?}", &path)
    }

    let name: String = path.to_string_lossy().into();
    let result = tokio::task::spawn_blocking(move || {
        // Read source image from file
        let reader = ImageReader::open(&path)?
            .with_guessed_format()?;
    
        let format = match reader.format() {
            Some(f) => f,
            None => bail!("Unable to detect image format from file."),
        };
        let img = reader.decode()?;
    
        // Create a Sha256 object and use image bytes as input
        let mut hasher = Sha256::new();
        hasher.input(img.as_bytes());
    
        debug!("Proceeding to resize {:?} image...", format);
    
        let width = match NonZeroU32::new(img.width()) {
            Some(w) => w,
            None => bail!("Failed to read width from image"),
        };
        let height = match NonZeroU32::new(img.height()) {
            Some(h) => h,
            None => bail!("Failed to read height from image"),
        };
    
        debug!("Resizing {:?} from width: {} and height: {}...", path, width, height);
    
        let scale_ref = match width > height {
            true => ScaleRef::Width(width.get()),
            false => ScaleRef::Height(height.get())
        };
    
        let src = SourceFile {
            width, height, scale: scale_ref, source_path: path,
            target_path: target, pixel: PixelType::U8x4,
            transparency: [ImageFormat::Png, ImageFormat::Gif, ImageFormat::WebP,
                ImageFormat::Bmp, ImageFormat::Tiff].contains(&format),
            sha256sum: hasher.result_str()
        };

        Ok((src, img))
    }).await?;
    
    let (src, img) = match result {
        Ok(t) => t,
        Err(e) => bail!("{}: Source image failed: {}", name, e),
    };

    // Create Resizer instance and resize source image
    // into buffer of destination image
    let s3 = Objects::from(&obj_store)?;
    let mut stats = ResizeStats::new();
    let mut resizer = Resizer::new(
        ResizeAlg::Convolution(FilterType::Lanczos3),
    );

    match src.transparency {
        true => {
            stats.extend(png_resize(&IMG_TARGET_RATIOS, &src, &img, &mut resizer, &s3).await);
            stats.extend(png_crop_resize(&THUMBNAIL_TARGET_RATIOS, &src, &img, &mut resizer, &s3).await);
        },
        false => {
            stats.extend(jpg_resize(&IMG_TARGET_RATIOS, &src, &img, &mut resizer, &s3).await);
            stats.extend(jpg_crop_resize(&THUMBNAIL_TARGET_RATIOS, &src, &img, &mut resizer, &s3).await);
        }
    }

    Ok(stats)
}
