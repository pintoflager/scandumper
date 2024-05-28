use image::imageops::{rotate180_in_place, rotate270, rotate90};
use image::{DynamicImage, RgbaImage};
use imageproc::drawing::draw_polygon_mut;

use super::*;


// Cut triangle pointing right
pub fn triangle_right(buf: &ImageBuffer<Rgba<u8>, Vec<u8>>, size: u32, transformable: &Transformable) -> (DynamicImage, Transformable) {
    let mut img = RgbaImage::from_pixel(size, size, TRANSPARENT);
    let div2 = (size / 2).try_into().unwrap();

    draw_polygon_mut(
        &mut img,
        &polygon_points(div2, 3),
        WHITE
    );

    substitute_color_px(&mut img, buf, WHITE);

    let (height, y) = cleanup_vertical(&img);
    let (width, x) = cleanup_horizontal(&img);

    let mut dyn_img = DynamicImage::ImageRgba8(img);
    let cropped_img = dyn_img.crop(x, y, width, height);
    
    let mut transformation = transformable.clone();
    transformation.set_dimensions(width, height);
    transformation.scale = ScaleRef::Width(width);

    (cropped_img, transformation)
}

// Cut triangle pointing left
pub fn triangle_left(buf: &ImageBuffer<Rgba<u8>, Vec<u8>>, size: u32, transformable: &Transformable) -> (DynamicImage, Transformable) {
    let mut img = RgbaImage::from_pixel(size, size, TRANSPARENT);
    let div2 = (size / 2).try_into().unwrap();

    draw_polygon_mut(
        &mut img,
        &polygon_points(div2, 3),
        WHITE
    );

    rotate180_in_place(&mut img);
    substitute_color_px(&mut img, buf, WHITE);

    let (height, y) = cleanup_vertical(&img);
    let (width, x) = cleanup_horizontal(&img);

    let mut dyn_img = DynamicImage::ImageRgba8(img);
    let cropped_img = dyn_img.crop(x, y, width, height);
    
    let mut transformation = transformable.clone();
    transformation.set_dimensions(width, height);
    transformation.scale = ScaleRef::Width(width);

    (cropped_img, transformation)
}

// Cut triangle pointing down
pub fn triangle_down(buf: &ImageBuffer<Rgba<u8>, Vec<u8>>, size: u32, transformable: &Transformable) -> (DynamicImage, Transformable) {
    let mut img = RgbaImage::from_pixel(size, size, TRANSPARENT);
    let div2 = (size / 2).try_into().unwrap();

    draw_polygon_mut(
        &mut img,
        &polygon_points(div2, 3),
        WHITE
    );

    let mut img = rotate90(&mut img);
    substitute_color_px(&mut img, buf, WHITE);

    let (height, y) = cleanup_vertical(&img);
    let (width, x) = cleanup_horizontal(&img);

    let mut dyn_img = DynamicImage::ImageRgba8(img);
    let cropped_img = dyn_img.crop(x, y, width, height);
    
    let mut transformation = transformable.clone();
    transformation.set_dimensions(width, height);
    transformation.scale = ScaleRef::Height(height);

    (cropped_img, transformation)
}

// Cut triangle pointing up
pub fn triangle_up(buf: &ImageBuffer<Rgba<u8>, Vec<u8>>, size: u32, transformable: &Transformable) -> (DynamicImage, Transformable) {
    let mut img = RgbaImage::from_pixel(size, size, TRANSPARENT);
    let div2 = (size / 2).try_into().unwrap();

    draw_polygon_mut(
        &mut img,
        &polygon_points(div2, 3),
        WHITE
    );

    let mut img = rotate270(&mut img);
    substitute_color_px(&mut img, buf, WHITE);

    let (height, y) = cleanup_vertical(&img);
    let (width, x) = cleanup_horizontal(&img);

    let mut dyn_img = DynamicImage::ImageRgba8(img);
    let cropped_img = dyn_img.crop(x, y, width, height);
    
    let mut transformation = transformable.clone();
    transformation.set_dimensions(width, height);
    transformation.scale = ScaleRef::Height(height);

    (cropped_img, transformation)
}
