use image::{DynamicImage, ImageBuffer, Rgba, RgbaImage};
use imageproc::drawing::draw_polygon_mut;

use super::*;


pub fn hexagonal_from_rect(buf: &ImageBuffer<Rgba<u8>, Vec<u8>>, size: u32, transformable: &Transformable) -> (DynamicImage, Transformable) {
    let div2 = (size / 2).try_into().unwrap();
    let mut img = RgbaImage::from_pixel(size, size, TRANSPARENT);

    draw_polygon_mut(
        &mut img,
        &polygon_points(div2, 6),
        WHITE
    );

    substitute_color_px(&mut img, buf, WHITE);

    // Remove rows where all pixels are transparent
    let (height, y) = cleanup_vertical(&img);
    
    let mut dyn_img = DynamicImage::ImageRgba8(img);
    
    // Crop hexagonal image to the smallest possible size (top and bottom has empty canvas)
    let cropped_dyn_img = dyn_img.crop(0, y, size, height);

    // Swap scale reference to use the height as the locked value and width as the moving one
    let mut transformable_hex = transformable.clone();
    transformable_hex.set_dimensions(size, height);
    transformable_hex.scale = ScaleRef::Width(size);

    (cropped_dyn_img, transformable_hex)
}

pub fn septagonal_from_rect(buf: &ImageBuffer<Rgba<u8>, Vec<u8>>, size: u32) -> DynamicImage {
    let div2 = (size / 2).try_into().unwrap();
    let mut img = RgbaImage::from_pixel(size, size, TRANSPARENT);

    draw_polygon_mut(
        &mut img,
        &polygon_points(div2, 7),
        WHITE
    );

    substitute_color_px(&mut img, buf, WHITE);
    
    DynamicImage::ImageRgba8(img)
}

pub fn sq45_from_rect(buf: &ImageBuffer<Rgba<u8>, Vec<u8>>, size: u32) -> DynamicImage {
    let div2 = (size / 2).try_into().unwrap();
    let mut img = RgbaImage::from_pixel(size, size, TRANSPARENT);

    // Cut 'tilted to 45 degrees' square image
    draw_polygon_mut(
        &mut img,
        &polygon_points(div2, 4),
        WHITE
    );

    substitute_color_px(&mut img, buf, WHITE);
    
    DynamicImage::ImageRgba8(img)
}

pub fn cross_from_rect(buf: &ImageBuffer<Rgba<u8>, Vec<u8>>, size: u32) -> DynamicImage {
    let div3 = (size / 3).try_into().unwrap();
    let div0 = size.try_into().unwrap();
    let mut img = RgbaImage::from_pixel(size, size, TRANSPARENT);

    draw_polygon_mut(
        &mut img,
        &[
            Point::new(div3, 0),
            Point::new(div3 * 2, 0),
            Point::new(div3 * 2, div3),
            Point::new(div0, div3),
            Point::new(div0, div3 * 2),
            Point::new(div3 * 2, div3 * 2),
            Point::new(div3 * 2, div0),
            Point::new(div3, div0),
            Point::new(div3, div3 * 2),
            Point::new(0, div3 * 2),
            Point::new(0, div3),
            Point::new(div3, div3),
        ],
        WHITE
    );

    substitute_color_px(&mut img, buf, WHITE);
    
    DynamicImage::ImageRgba8(img)
}

pub fn star_from_rect(buf: &ImageBuffer<Rgba<u8>, Vec<u8>>, size: u32, transformable: &Transformable) -> (DynamicImage, Transformable) {
    let div2: i32 = (size / 2).try_into().unwrap();
    
    let mut img = RgbaImage::from_pixel(size, size, TRANSPARENT);
    
    // Distances between 10 points of the star, 5 inner and 5 outer on a circle
    let star_point_dist = (2.0 * PI) / 10.0;
    let mut points = vec![];

    // We don't want to multiply by zero so start from 1
    for i in 1..11 {
        // Every other point is 'drawn in' while the next one has the 'real' radius,
        // the star's sharp ends.
        let r = match i % 2 == 1 {
            true => div2,
            false => div2 / 2
        };

        // Rotate around the circle, multiplying point's index (i) with the distance
        // between the points on the circle
        let rotation = star_point_dist * i as f32;

        // Get X and Y points with some dark mathemathics
        let mut x = r as f32 * rotation.sin();
        let mut y = r as f32 * rotation.cos();

        // Both are off by r length (center the star in the image)
        x += div2 as f32;
        y += div2 as f32;

        points.push(Point::new(x.floor() as i32, y.floor() as i32));
    }

    draw_polygon_mut(
        &mut img,
        &points,
        WHITE
    );

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
