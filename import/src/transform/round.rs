use image::{DynamicImage, ImageBuffer, Rgba, RgbaImage};
use imageproc::drawing::draw_filled_circle_mut;

use super::*;


pub fn round_from_rect(buf: &ImageBuffer<Rgba<u8>, Vec<u8>>, size: u32) -> DynamicImage {
    // Draw a white circle on a black background that is the same size as the cropped image
    let div2 = (size / 2).try_into().unwrap();
    let mut img = RgbaImage::from_pixel(size, size, TRANSPARENT);
    
    draw_filled_circle_mut(
        &mut img,
        (div2, div2),
        div2,
        WHITE
    );

    substitute_color_px(&mut img, buf, WHITE);
    
    DynamicImage::ImageRgba8(img)
}
