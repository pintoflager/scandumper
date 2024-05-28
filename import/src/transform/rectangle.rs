use image::{DynamicImage, ImageBuffer, Rgba, RgbaImage};
use imageproc::{drawing::draw_filled_rect_mut, rect::Rect};

use super::*;


pub fn multi_rect_horizontal(buf: &ImageBuffer<Rgba<u8>, Vec<u8>>, size: u32, count: u32) -> DynamicImage {
    // Well, stupid
    if count <= 1 {
        return DynamicImage::ImageRgba8(RgbaImage::from_pixel(size, size, TRANSPARENT));
    }

    // For 2 and 3 row outputs form a different padding
    let padding_sizer = match count {
        2 => count + 2,
        3 => count + 1,
        _ => count,
    };
    
    // Divide image height with requested row count
    let div = size / count;

    // Set padding by dividing the row height with the resizer defined above
    let padding = div / padding_sizer;

    // Margin between rows is the row count minus 2 for 4 and more rows, below that
    // we should prevent divide by zero (or one which doesn't make any sense)
    let margin_sizer = match count {
        2 | 3 => count - 1,
        _ => count - 2,
    };

    // Margin between rows is padding divided by the 'visible row separators' which
    // usually is 2 less than the row count. See margin_sizer above.
    let margin = (padding / margin_sizer) + div;

    // Final height of the row with padding subtracted
    let height = div - padding;
    let mut img = RgbaImage::from_pixel(size, size, TRANSPARENT);
    
    // Iterate rows and draw white rectangles to substitute later on.
    for i in 0..count {
        let y = i * margin;
        let rect = Rect::at(0, y as i32).of_size(size, height);

        draw_filled_rect_mut(&mut img, rect, WHITE);
    }

    // Substitute white rectangles with the original image pixels
    substitute_color_px(&mut img, buf, WHITE);
    
    DynamicImage::ImageRgba8(img)
}
