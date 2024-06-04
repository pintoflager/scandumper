
use std::path::PathBuf;
use anyhow::{bail, Result};
use config::{Config, ObjectStore};
use fast_image_resize::{ResizeOptions, SrcCropping};
use fast_image_resize::PixelType;

use super::resize::jpeg::*;
use super::resize::png::*;
use crate::transform::{
    cross_from_rect, hexagonal_from_rect, multi_rect_horizontal, round_from_rect, septagonal_from_rect,
    sq45_from_rect, star_from_rect, transformable_img, triangle_down, triangle_left, triangle_right,
    triangle_up, ScaleRef
};
use crate::{resize_handler, ResizeStats, TargetSize};


pub async fn resize_action(importable: PathBuf, target_path: PathBuf, config: Config, fs_root: Option<PathBuf>, s3_store: Option<ObjectStore>)
-> Result<ResizeStats> {
    let (transformable, img) = transformable_img(importable, target_path).await?;

    // Create Resizer instance and resize source image
    // into buffer of destination image
    let mut stats = ResizeStats::new();
    
    // Resize options for normal size images
    let resize_opts = Some(ResizeOptions::new());

    // Target sizes
    let og = TargetSize::original(&config);
    let xl = TargetSize::xl(&config);
    let lg = TargetSize::lg(&config);
    let md = TargetSize::md(&config);
    let sm = TargetSize::sm(&config);
    let xs = TargetSize::xs(&config);

    // Resize options for cropped 'md' size images
    let crop_opts_md = {
        let mut o = ResizeOptions::new();
        o.cropping = SrcCropping::FitIntoDestination((
            md.to_px() as f64,
            md.to_px() as f64
        ));
        Some(o)
    };

    // Resize options for cropped 'sm' size images
    let crop_opts_sm = {
        let mut o = ResizeOptions::new();
        o.cropping = SrcCropping::FitIntoDestination((
            sm.to_px() as f64,
            sm.to_px() as f64
        ));
        Some(o)
    };

    // Resize options for cropped 'xs' size images a.k.a thumbnails
    let crop_opts_xs = {
        let mut o = ResizeOptions::new();
        o.cropping = SrcCropping::FitIntoDestination((
            xs.to_px() as f64,
            xs.to_px() as f64
        ));
        Some(o)
    };

    // Create grayscale PNG image
    let gray_img = img.grayscale();
    let mut transformable_gray = transformable.clone();
    transformable_gray.target_path = {
        let mut p = transformable_gray.target_path.clone();
        p.push("gray");
        p
    };

    // Sizes that should not be cropped
    let resize_multiple = [
        (og.to_px(), og.to_str()),
        (xl.to_px(), xl.to_str()),
        (lg.to_px(), lg.to_str()),
    ];

    // ..sizes that will be cropped
    let md_ratio = [(md.to_px(), md.to_str())];
    let sm_ratio = [(sm.to_px(), sm.to_str())];
    let xs_ratio = [(xs.to_px(), xs.to_str())];

    match transformable.target_ext {
        "png" => {
            // Normal resize for PNG images
            let handler = resize_handler(
                &resize_multiple,
                &transformable,
                &img,
                &resize_opts,
                &s3_store,
                &fs_root,
                png_writer,
            );
            stats.extend(handler.await);

            // Set transparent pixel for grayscale PNG image
            transformable_gray.pixel = PixelType::U8x2;

            // Crop and resize 'md' size PNG image
            let handler = resize_handler(
                &md_ratio,
                &transformable,
                &img,
                &crop_opts_md,
                &s3_store,
                &fs_root,
                png_writer,
            );
            stats.extend(handler.await);

            // Crop and resize 'md' size grayscale PNG image
            let handler = resize_handler(
                &md_ratio,
                &transformable_gray,
                &gray_img,
                &crop_opts_md,
                &s3_store,
                &fs_root,
                png_gray_writer,
            );
            stats.extend(handler.await);
            
            // Crop and resize 'sm' size PNG image
            let handler = resize_handler(
                &sm_ratio,
                &transformable,
                &img,
                &crop_opts_sm,
                &s3_store,
                &fs_root,
                png_writer,
            );
            stats.extend(handler.await);

            // Crop and resize 'sm' size grayscale PNG image
            let handler = resize_handler(
                &sm_ratio,
                &transformable_gray,
                &gray_img,
                &crop_opts_sm,
                &s3_store,
                &fs_root,
                png_gray_writer,
            );
            stats.extend(handler.await);

            // Crop and resize 'xs' size PNG image
            let handler = resize_handler(
                &xs_ratio,
                &transformable,
                &img,
                &crop_opts_xs,
                &s3_store,
                &fs_root,
                png_writer,
            );
            stats.extend(handler.await);

            // Crop and resize 'xs' size grayscale PNG image
            let handler = resize_handler(
                &xs_ratio,
                &transformable_gray,
                &gray_img,
                &crop_opts_xs,
                &s3_store,
                &fs_root,
                png_gray_writer,
            );
            stats.extend(handler.await);
        },
        "jpeg" => {
            // Normal resize for JPEG images
            let handler = resize_handler(
                &resize_multiple,
                &transformable,
                &img,
                &resize_opts,
                &s3_store,
                &fs_root,
                jpeg_writer,
            );
            stats.extend(handler.await);

            // Set pixel for grayscale PNG image
            transformable_gray.pixel = PixelType::U8;

            // Crop and resize 'md' size JPEG image
            let handler = resize_handler(
                &md_ratio,
                &transformable,
                &img,
                &crop_opts_md,
                &s3_store,
                &fs_root,
                jpeg_writer,
            );
            stats.extend(handler.await);

            // Crop and resize 'md' size grayscale JPEG image
            let handler = resize_handler(
                &md_ratio,
                &transformable_gray,
                &gray_img,
                &crop_opts_md,
                &s3_store,
                &fs_root,
                jpeg_gray_writer,
            );
            stats.extend(handler.await);

            // Crop and resize 'sm' size JPEG image
            let handler = resize_handler(
                &sm_ratio,
                &transformable,
                &img,
                &crop_opts_sm,
                &s3_store,
                &fs_root,
                jpeg_writer,
            );
            stats.extend(handler.await);

            // Crop and resize 'sm' size grayscale JPEG image
            let handler = resize_handler(
                &sm_ratio,
                &transformable_gray,
                &gray_img,
                &crop_opts_sm,
                &s3_store,
                &fs_root,
                jpeg_gray_writer,
            );
            stats.extend(handler.await);

            // Crop and resize 'xs' size JPEG image
            let handler = resize_handler(
                &xs_ratio,
                &transformable,
                &img,
                &crop_opts_xs,
                &s3_store,
                &fs_root,
                jpeg_writer,
            );
            stats.extend(handler.await);

            // Crop and resize 'xs' size grayscale JPEG image
            let handler = resize_handler(
                &xs_ratio,
                &transformable_gray,
                &gray_img,
                &crop_opts_xs,
                &s3_store,
                &fs_root,
                jpeg_gray_writer,
            );
            stats.extend(handler.await);
        },
        _ => bail!("Unsupported image format: {:?}", transformable.target_ext),
    }

    Ok(stats)
}

pub async fn transform_action(importable: PathBuf, target_path: PathBuf, size: TargetSize, fs_root: Option<PathBuf>,
s3_store: Option<ObjectStore>)
-> Result<ResizeStats> {
    let (mut transformable, img) = transformable_img(
        importable,
        target_path
    ).await?;

    // Since we use already resized image as a source our target paths and names are all cocked up
    let mut target_path = transformable.target_path.clone();

    // Has the 'target size' as the parent element in the path, take it away
    target_path.pop();

    // Add shapes dir to collect all transformed images
    target_path.push("shapes");
    transformable.target_path = target_path;

    // Create Resizer instance and resize source image
    // into buffer of destination image
    let mut stats = ResizeStats::new();
    
    // Resize options for normal size images
    let resize_opts = Some(ResizeOptions::new());

    // Crop image into a max sized square for transformations that expect a square image
    let (rect_side, x, y) = match img.width() >= img.height() {
        true => (
            img.height(),
            (img.width() - img.height()) / 2,
            0
        ),
        false => (
            img.width(),
            0,
            (img.height() - img.width()) / 2,
        )
    };
    
    // We cropped the input image so transformable should be updated
    transformable.set_dimensions(rect_side, rect_side);
    transformable.scale = ScaleRef::Fixed(rect_side, rect_side);
    transformable.pixel = PixelType::U8x4;
    transformable.target_ext = "png";
    
    // Create dynamic square image and read its contents to buffer
    let img_square = img.clone();
    let img_square_buf = img_square.crop_imm(x, y, rect_side, rect_side).to_rgba8();

    // Crop and resize 'sm' size ROUND PNG image
    let img_circle = round_from_rect(&img_square_buf, rect_side);
    let ratio = [(size.to_px(), "round")];

    let handler = resize_handler(
        &ratio,
        &transformable,
        &img_circle,
        &resize_opts,
        &s3_store,
        &fs_root,
        png_writer,
    );
    stats.extend(handler.await);

    // Cut hexagonal image from the square image
    let (cropped_hex, img_hex_transf) = hexagonal_from_rect(&img_square_buf, rect_side, &transformable);
    let ratio = [(size.to_px(), "hex")];

    let handler = resize_handler(
        &ratio,
        &img_hex_transf,
        &cropped_hex,
        &resize_opts,
        &s3_store,
        &fs_root,
        png_writer,
    );
    stats.extend(handler.await);

    // Cut septagonal image from the square image
    let cropped_sep = septagonal_from_rect(&img_square_buf, rect_side);
    let ratio = [(size.to_px(), "sep")];

    let handler = resize_handler(
        &ratio,
        &transformable,
        &cropped_sep,
        &resize_opts,
        &s3_store,
        &fs_root,
        png_writer,
    );
    stats.extend(handler.await);

    // Crop and resize 'sm' size 45 DEGREE ANGLE TILTED square PNG image
    let cropped_sq45 = sq45_from_rect(&img_square_buf, rect_side);
    let ratio = [(size.to_px(), "sq45")];

    let handler = resize_handler(
        &ratio,
        &transformable,
        &cropped_sq45,
        &resize_opts,
        &s3_store,
        &fs_root,
        png_writer,
    );
    stats.extend(handler.await);

    // Crop and resize 'sm' size triangle RIGHT PNG image
    let (cropped_triangle, transf_triangle) = triangle_right(&img_square_buf, rect_side, &transformable);
    let ratio = [(size.to_px(), "right")];

    let handler = resize_handler(
        &ratio,
        &transf_triangle,
        &cropped_triangle,
        &resize_opts,
        &s3_store,
        &fs_root,
        png_writer,
    );
    stats.extend(handler.await);

    // Crop and resize 'sm' size triangle LEFT PNG image
    let (cropped_triangle, transf_triangle) = triangle_left(&img_square_buf, rect_side, &transformable);
    let ratio = [(size.to_px(), "left")];

    let handler = resize_handler(
        &ratio,
        &transf_triangle,
        &cropped_triangle,
        &resize_opts,
        &s3_store,
        &fs_root,
        png_writer,
    );
    stats.extend(handler.await);

    // Crop and resize 'sm' size triangle DOWN PNG image
    let (cropped_triangle, transf_triangle) = triangle_down(&img_square_buf, rect_side, &transformable);
    let ratio = [(size.to_px(), "down")];

    let handler = resize_handler(
        &ratio,
        &transf_triangle,
        &cropped_triangle,
        &resize_opts,
        &s3_store,
        &fs_root,
        png_writer,
    );
    stats.extend(handler.await);

    // Crop and resize 'sm' size triangle UP PNG image
    let (cropped_triangle, transf_triangle) = triangle_up(&img_square_buf, rect_side, &transformable);
    let ratio = [(size.to_px(), "up")];

    let handler = resize_handler(
        &ratio,
        &transf_triangle,
        &cropped_triangle,
        &resize_opts,
        &s3_store,
        &fs_root,
        png_writer,
    );
    stats.extend(handler.await);

    // Crop and resize 'sm' size 2 HORIZONTAL RECTANGLES PNG image
    let cropped_rect = multi_rect_horizontal(&img_square_buf, rect_side, 2);
    let ratio = [(size.to_px(), "row2")];

    let handler = resize_handler(
        &ratio,
        &transformable,
        &cropped_rect,
        &resize_opts,
        &s3_store,
        &fs_root,
        png_writer,
    );
    stats.extend(handler.await);

    // Crop and resize 'sm' size 3 HORIZONTAL RECTANGLES PNG image
    let cropped_rect = multi_rect_horizontal(&img_square_buf, rect_side, 3);
    let ratio = [(size.to_px(), "row3")];

    let handler = resize_handler(
        &ratio,
        &transformable,
        &cropped_rect,
        &resize_opts,
        &s3_store,
        &fs_root,
        png_writer,
    );
    stats.extend(handler.await);
    
        // Crop and resize 'sm' size 4 HORIZONTAL RECTANGLES PNG image
        let cropped_rect = multi_rect_horizontal(&img_square_buf, rect_side, 4);
        let ratio = [(size.to_px(), "row4")];
    
        let handler = resize_handler(
            &ratio,
            &transformable,
            &cropped_rect,
            &resize_opts,
            &s3_store,
            &fs_root,
            png_writer,
        );
        stats.extend(handler.await);

    // Crop and resize 'sm' size CROSS PNG image
    let cropped_cross = cross_from_rect(&img_square_buf, rect_side);
    let ratio = [(size.to_px(), "cross")];

    let handler = resize_handler(
        &ratio,
        &transformable,
        &cropped_cross,
        &resize_opts,
        &s3_store,
        &fs_root,
        png_writer,
    );
    stats.extend(handler.await);

    // Crop and resize 'sm' size STAR PNG image
    let (cropped_star, transf_star) = star_from_rect(&img_square_buf, rect_side, &transformable);
    let ratio = [(size.to_px(), "star")];

    let handler = resize_handler(
        &ratio,
        &transf_star,
        &cropped_star,
        &resize_opts,
        &s3_store,
        &fs_root,
        png_writer,
    );
    stats.extend(handler.await);

    Ok(stats)
}
