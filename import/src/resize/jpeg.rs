use fast_image_resize::images::Image;
use image::codecs::jpeg::JpegEncoder;
use image::{ExtendedColorType, ImageEncoder};
use anyhow::{Result, anyhow};


pub async fn jpeg_writer(image: Image<'static>) -> Result<Vec<u8>> {
    let mut buf = vec![];

    JpegEncoder::new(&mut buf).write_image(
        image.buffer(),
        image.width(),
        image.height(),
        ExtendedColorType::Rgb8,
    ).map_err(|e| anyhow!("Failed to create JPEG image: {}", e))
    .map(|_| buf)
}

pub async fn jpeg_gray_writer(image: Image<'static>) -> Result<Vec<u8>> {
    let mut buf = vec![];

    JpegEncoder::new(&mut buf).write_image(
        image.buffer(),
        image.width(),
        image.height(),
        ExtendedColorType::L8,
    ).map_err(|e| anyhow!("Failed to create grayscale JPEG image: {}", e))
    .map(|_| buf)
}
