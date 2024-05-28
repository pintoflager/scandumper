mod polygons;
mod triangles;
mod round;
mod rectangle;

use std::{f32::consts::PI, num::NonZeroU32, path::PathBuf};
use image::{io::Reader as ImageReader, DynamicImage};

use adler::adler32_slice;
use anyhow::{anyhow, bail, Result};
use config::ObjectStore;
use fast_image_resize::PixelType;
use image::{ImageBuffer, ImageFormat, Rgba};
use imageproc::point::Point;
use tracing::{debug, error, info, warn};

pub use triangles::*;
pub use round::*;
pub use polygons::*;
pub use rectangle::*;


pub const TRANSPARENT: Rgba<u8> = image::Rgba::<u8>([0, 0, 0, 0]);
pub const WHITE: Rgba<u8> = image::Rgba::<u8>([255, 255, 255, 100]);

#[derive(Clone, Debug)]
pub enum ScaleRef {
    Width(u32),
    Height(u32),
    Fixed(u32, u32)
}

#[derive(Clone, Debug)]
pub struct Transformable {
    pub width: NonZeroU32,
    pub height: NonZeroU32,
    pub scale: ScaleRef,
    pub source_path: PathBuf,
    pub target_path: PathBuf,
    pub target_name: String,
    pub target_ext: &'static str,
    pub pixel: PixelType,
    pub checksum: u32,
}

impl Transformable {
    pub fn set_dimensions(&mut self, w: u32, h: u32) {
        self.width = match NonZeroU32::new(w) {
            Some(v) => v,
            None => panic!("Width can't be zero"),
        };

        self.height = match NonZeroU32::new(h) {
            Some(v) => v,
            None => panic!("Height can't be zero"),
        };
    }
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
            },
            ScaleRef::Fixed(w, h) => match w >= r && h >= r {
                true => (r, r),
                false => match w >= h {
                    true => (h, h),
                    false => (w, w),
                }
            }
        }
    }
    pub async fn get_resizables(&self, items: &[(u32, &str)], s3: &Option<ObjectStore>, fs: &Option<PathBuf>)
    -> Vec<(u32, String, PathBuf)> {
        let mut resizables = vec![];

        for (ratio, id) in items {
            if !self.skip_duplicate(*id, s3, fs).await {
                resizables.push((*ratio, id.to_string(), self.target_file_path(id)));
            }
        }

        resizables
    }
    async fn skip_duplicate(&self, id: &str, s3: &Option<ObjectStore>, fs: &Option<PathBuf>) -> bool {
        let file_path = self.target_file_path(id);

        // See if the object already exists on S3
        let s3_is_duplicate = match s3 {
            Some(s) => {
                let path = file_path.to_string_lossy();
        
                match s.get_tags(path).await {
                    Ok(v) => match v.is_empty() {
                        true => {
                            debug!("Object {:?} does not exist / doesn't have tags", id);
                            false
                        },
                        false => match v.into_iter().find(|t|t.key().eq("checksum")) {
                            Some(t) => match t.value().eq(&self.checksum.to_string()) {
                                true => {
                                    debug!("Object {:?} is same as the provided item, skipping...", id);
                                    true
                                },
                                false => {
                                    info!("Object {:?} has changed, overwrite...", id);
                                    false
                                }
                            },
                            None => {
                                warn!("Object {:?} is missing checksum tag", id);
                                false
                            },
                        }
                    },
                    Err(e) => {
                        error!("Duplicate check failed: {}, weird.", e);
                        false
                    }
                }
            },
            None => false,
        };
        
        // Bail as duplicate if filesystem exporting is disabled
        if s3_is_duplicate && fs.is_none() {
            return s3_is_duplicate
        }

        // See if the object already exists on filesystem
        if fs.is_some() {
            // Check if the file exists on filesystem
            if file_path.is_file() {
                // Checksum file is next to the image
                let checksum_file = self.checksum_file_path(id);

                // Can't compare image without checksum file, overwrite
                if !checksum_file.is_file() {
                    return false
                }

                // Read checksum from file
                match std::fs::read_to_string(checksum_file) {
                    Ok(c) => match c.trim().parse::<u32>() {
                        Ok(v) => match v.eq(&self.checksum) {
                            true => {
                                debug!("File {:?} already exists on filesystem", id);
                                return true
                            },
                            false => {
                                info!("File {:?} has changed, overwrite...", id);
                                return false
                            }
                        },
                        Err(e) => {
                            error!("Failed to parse checksum from file: {:?}", e);
                            return false
                        }
                    },
                    Err(e) => {
                        error!("Failed to read checksum file: {:?}", e);
                        return false
                    }
                }
            }
        }        

        false
    }
    fn target_file_path(&self, id: &str) -> PathBuf {
        let mut path = self.target_path.to_owned();
        let ext = self.target_ext;

        let filename = format!("{}.{}", id, ext);
        path.push(filename);

        path
    }
    fn checksum_file_path(&self, id: &str) -> PathBuf {
        let mut path = self.target_path.to_owned();
        let filename = format!(".{}.checksum", id);
        
        path.push(filename);

        path
    }
}

pub async fn transformable_img(importable: PathBuf, mut target_path: PathBuf) -> Result<(Transformable, DynamicImage)> {
    if !importable.is_file() {
        bail!("Stupid developer issue, image resizer fed with a non file: {:?}", &importable)
    }

    let name: String = importable.to_string_lossy().into();
    
    tokio::task::spawn_blocking(move || {
        // Read source image from file
        let reader = ImageReader::open(&importable)?.with_guessed_format()?;
        let format = match reader.format() {
            Some(f) => f,
            None => bail!("Unable to detect image format from file."),
        };

        // Image is valid image
        let img = reader.decode()?;
    
        // Calculate checksum from image bytes
        let checksum = adler32_slice(img.as_bytes());
    
        // Read image width and height into non-zero enum to avoid problems later on
        let width = match NonZeroU32::new(img.width()) {
            Some(w) => w,
            None => bail!("Failed to read width from image"),
        };

        let height = match NonZeroU32::new(img.height()) {
            Some(h) => h,
            None => bail!("Failed to read height from image"),
        };
    
        debug!("Resizer received {:?} image {:?} from width: {} and height: {}", format, importable, width, height);
        
        // Portrait or landscape
        let scale_ref = match width > height {
            true => ScaleRef::Width(width.get()),
            false => ScaleRef::Height(height.get())
        };

        // Target name is the filename without extension
        let target_name = match importable.file_stem() {
            Some(f) => match f.to_str() {
                Some(s) => s.to_string(),
                None => bail!("Failed to extract filename from source image path"),
            },
            None => bail!("Failed to extract filename from source image path")
        };

        // Default target path should contain the target name
        target_path.push(&target_name);

        // For formats that suppot transparency, export as PNG, else JPEG
        let (target_ext, pixel) = match format {
            ImageFormat::Png |
            ImageFormat::Gif |
            ImageFormat::WebP |
            ImageFormat::Bmp |
            ImageFormat::Tiff => ("png", PixelType::U8x4),
            _ => ("jpeg", PixelType::U8x3),
        };
    
        let exportable = Transformable {
            width,
            height,
            scale: scale_ref,
            source_path: importable,
            target_path: target_path,
            target_name,
            target_ext,
            pixel,
            checksum,
        };

        Ok((exportable, img))
    })
    .await
    .map_err(|e|anyhow!("{}: Source image failed to load: {}", name, e))?
}

pub fn polygon_points(r: i32, sides: u32) -> Vec<Point<i32>>{
    let s = sides as f32;
    let mut points = vec![];
    
    for i in 0..sides {
        let rotation = ((2.0 * PI) / s) * i as f32;

        // Get x and y points
        let mut x = r as f32 * rotation.cos();
        let mut y = r as f32 * rotation.sin();
        
        // Both are off by r length
        x += r as f32;
        y += r as f32;

        points.push(Point::new(x.floor() as i32, y.floor() as i32));
    }

    points
}

pub fn substitute_color_px(target: &mut ImageBuffer<Rgba<u8>, Vec<u8>>, source: &ImageBuffer<Rgba<u8>, Vec<u8>>, color: Rgba<u8>) {
    for (x, y, p) in target.enumerate_pixels_mut() {
        if color.eq(*&p) {
            p.0 = source.get_pixel(x, y).0;
        }
    }
}

pub fn cleanup_vertical(img: &ImageBuffer<Rgba<u8>, Vec<u8>>) -> (u32, u32) {
    let mut y = 0;
    let mut height = 0;

    for mut p in img.rows() {
        if p.all(|i|TRANSPARENT.eq(*&i)) {
            match height == 0 {
                true => y += 1,
                false => break,
            };

            continue
        }

        height += 1;
    }

    (height, y)
}

pub fn cleanup_horizontal(img: &ImageBuffer<Rgba<u8>, Vec<u8>>) -> (u32, u32) {
    let mut left = usize::MAX;
    let mut right = 0;

    for p in img.rows() {
        for (u, i) in p.enumerate() {
            if !TRANSPARENT.eq(*&i) {
                if u < left {
                    left = u
                }

                if u > right {
                    right = u
                }
            }
        }

    }

    ((right - left) as u32, left as u32)
}
