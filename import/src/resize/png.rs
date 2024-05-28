use fast_image_resize::images::Image;
use image::codecs::png::PngEncoder;
use image::{ExtendedColorType, ImageEncoder};
use anyhow::{Result, anyhow};


pub async fn png_writer(image: Image<'static>) -> Result<Vec<u8>> {
    let mut buf = vec![];

    PngEncoder::new(&mut buf).write_image(
        image.buffer(),
        image.width(),
        image.height(),
        ExtendedColorType::Rgba8,
    ).map_err(|e| anyhow!("Failed to create PNG image: {}", e))
    .map(|_| buf)
}

pub async fn png_gray_writer(image: Image<'static>) -> Result<Vec<u8>> {
    let mut buf = vec![];

    PngEncoder::new(&mut buf).write_image(
        image.buffer(),
        image.width(),
        image.height(),
        ExtendedColorType::La8,
    ).map_err(|e| anyhow!("Failed to create grayscale PNG image: {}", e))
    .map(|_| buf)
}
