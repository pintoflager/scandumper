use std::num::NonZeroU32;
use std::path::PathBuf;
use fast_image_resize::{Image, Resizer, MulDiv};
use image::codecs::png::PngEncoder;
use image::{ColorType, ImageEncoder, DynamicImage};
use anyhow::{Result, bail, anyhow};
use tokio::task::JoinSet;
use tracing::{info, debug};

use super::object_store::Objects;
use super::{SourceFile, ResizeStats};

pub async fn png_resize(ratios: &[(u32, &str)], src: &SourceFile, img: &DynamicImage,
resizer: &mut Resizer, s3: &Objects) -> Result<ResizeStats> {
    let resizables = src.get_resizables(ratios, s3, "png").await?;
    let mut stats = ResizeStats::new();
    
    if resizables.is_empty() {
        stats.skipped.push(format!("Image {:?} already exists on S3", src.source_path));

        return Ok(stats)
    }

    let mut src_image = Image::from_vec_u8(
        src.width,
        src.height,
        img.to_rgba8().into_raw(),
        src.pixel,
    )?;

    // Multiple RGB channels of source image by alpha channel 
    // (not required for the Nearest algorithm)
    let alpha_mul_div = MulDiv::default();
    alpha_mul_div.multiply_alpha_inplace(&mut src_image.view_mut())?;
    let mut handles = JoinSet::new();

    for (r, s, p) in resizables {
        let (w, h) = src.convert_ratio(r);

        // Create container for data of destination image
        let dst_width = match NonZeroU32::new(w) {
            Some(w) => w,
            None => bail!("Failed to set resize width for image"),
        };
        let dst_height = match NonZeroU32::new(h) {
            Some(h) => h,
            None => bail!("Failed to set resize height for image"),
        };
        
        debug!("Resizing {:?} {} to width: {} and height: {}...", p, s.to_uppercase(),
            dst_width, dst_height);

        let mut dst_image = Image::new(
            dst_width,
            dst_height,
            src.pixel,
        );
    
        // Get mutable view of destination image data
        let mut dst_view = dst_image.view_mut();

        // Resize source image into buffer of destination image
        resizer.resize(&src_image.view(), &mut dst_view)?;
    
        // Divide RGB channels of destination image by alpha
        alpha_mul_div.divide_alpha_inplace(&mut dst_view)?;
    
        let sha = src.sha256sum.to_owned();
        // Create new bucket for each task: https://github.com/durch/rust-s3/issues/337
        let s3 = Objects::from(&s3.obj_store)?;

        handles.spawn(async move {
            // Write destination image as jpeg file
            png_writer(&p, dst_image, &s3).await?;
    
            s3.tag_source(&sha, &p).await
        });
    }

    while let Some(r) = handles.join_next().await {
        stats.push(r?);
    }

    Ok(stats)
}

pub async fn png_crop_resize(ratios: &[(u32, &str)], src: &SourceFile, img: &DynamicImage,
resizer: &mut Resizer, s3: &Objects) -> Result<ResizeStats> {
    let resizables = src.get_resizables(ratios, s3, "png").await?;
    let mut stats = ResizeStats::new();
    
    if resizables.is_empty() {
        stats.skipped.push(format!("Image {:?} already exists on S3", src.source_path));

        return Ok(stats)
    }

    let src_image = Image::from_vec_u8(
        src.width,
        src.height,
        img.to_rgba8().into_raw(),
        src.pixel,
    )?;

    // Set cropping parameters
    let mut view = src_image.view();
    let mut handles = JoinSet::new();

    for (r, s, p) in resizables {
        // Create container for data of destination image
        let dst_width = match NonZeroU32::new(r) {
            Some(w) => w,
            None => bail!("Failed to set resize width for image"),
        };
        let dst_height = match NonZeroU32::new(r) {
            Some(h) => h,
            None => bail!("Failed to set resize height for image"),
        };
        
        debug!("Resizing and cropping {:?} {} to width: {} and height: {}...", p, s.to_uppercase(),
            dst_width, dst_height);

        view.set_crop_box_to_fit_dst_size(dst_width, dst_height, None);

        // Create container for data of destination image
        let mut dst_image = Image::new(
            dst_width,
            dst_height,
            src.pixel,
        );
        // Get mutable view of destination image data
        let mut dst_view = dst_image.view_mut();

        // Resize source image into buffer of destination image
        resizer.resize(&view, &mut dst_view)?;

        let sha = src.sha256sum.to_owned();
        // Create new bucket for each task: https://github.com/durch/rust-s3/issues/337
        let s3 = Objects::from(&s3.obj_store)?;

        handles.spawn(async move {
            // Write destination image as jpeg file
            png_writer(&p, dst_image, &s3).await?;
    
            s3.tag_source(&sha, &p).await
        });
    }

    while let Some(r) = handles.join_next().await {
        stats.push(r?);
    }

    Ok(stats)
}

async fn png_writer(file_path: &PathBuf, image: Image<'_>, s3: &Objects) -> Result<()> {
    let content_type = "image/png";
    let s3_path = file_path.to_string_lossy();
    let mut buf = vec![];

    PngEncoder::new(&mut buf).write_image(
        image.buffer(),
        image.width().get(),
        image.height().get(),
        ColorType::Rgba8,
    ).map_err(|e| anyhow!("{}: Failed to write image: {}", s3_path.as_ref(), e))?;

    match s3.bucket.put_object_with_content_type(s3_path.as_ref(), &buf, content_type).await {
        Ok(r) => {
            info!("{}: S3 object OK ({})", s3_path.as_ref(), r.status_code());
            Ok(())
        },
        Err(e) => bail!("{}: Failed to store S3 object: {}", s3_path.as_ref(), e),
    }
}
