use image::{DynamicImage, GenericImageView};

pub const MAX_IMAGE_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_IMAGE_WIDTH: u32 = 4096;
pub const MAX_IMAGE_HEIGHT: u32 = 4096;
pub const MAX_ALLOC_BYTES: u64 = 64 * 1024 * 1024;
pub const MIN_ARTWORK_SIDE: u32 = 200;
pub const HQ_SOURCE_SIDE: u32 = 0;

pub fn decode_artwork(bytes: &[u8]) -> Result<DynamicImage, String> {
    if bytes.len() > MAX_IMAGE_BYTES {
        return Err("Artwork byte size exceeds limit".into());
    }

    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes));
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_IMAGE_WIDTH);
    limits.max_image_height = Some(MAX_IMAGE_HEIGHT);
    limits.max_alloc = Some(MAX_ALLOC_BYTES);
    reader.limits(limits);
    let reader = reader
        .with_guessed_format()
        .map_err(|e| format!("Cannot detect artwork format: {e}"))?;
    let image = reader
        .decode()
        .map_err(|e| format!("Cannot decode artwork: {e}"))?;

    let (width, height) = image.dimensions();

    if width > MAX_IMAGE_WIDTH || height > MAX_IMAGE_HEIGHT {
        return Err(format!(
            "Artwork dimensions ({width}x{height}) exceed maximum allowed ({MAX_IMAGE_WIDTH}x{MAX_IMAGE_HEIGHT})"
        ));
    }

    if width < MIN_ARTWORK_SIDE || height < MIN_ARTWORK_SIDE {
        return Err(format!(
            "Artwork too small ({width}x{height}), likely a placeholder"
        ));
    }

    if is_gray_placeholder(&image) {
        return Err("Detected YouTube placeholder thumbnail".into());
    }

    let image = if width > height {
        let left = (width - height) / 2;
        image.crop_imm(left, 0, height, height)
    } else {
        image
    };

    Ok(image)
}

pub fn is_gray_placeholder(image: &DynamicImage) -> bool {
    let (w, h) = image.dimensions();
    if w == 0 || h == 0 {
        return true;
    }

    let steps = 8u32;
    let dx = (w / steps).max(1);
    let dy = (h / steps).max(1);
    let mut min_luma = u8::MAX;
    let mut max_luma = u8::MIN;

    for sy in 0..steps {
        for sx in 0..steps {
            let x = (sx * dx).min(w - 1);
            let y = (sy * dy).min(h - 1);
            let [r, g, b, _] = image.get_pixel(x, y).0;
            let hi = r.max(g).max(b);
            let lo = r.min(g).min(b);
            if hi - lo > 2 {
                return false;
            }
            min_luma = min_luma.min(lo);
            max_luma = max_luma.max(hi);
        }
    }

    max_luma.saturating_sub(min_luma) <= 16
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageFormat, Rgb, RgbImage};
    use std::io::Cursor;

    fn encode_png(img: RgbImage) -> Vec<u8> {
        let mut buf = Cursor::new(Vec::new());
        DynamicImage::ImageRgb8(img)
            .write_to(&mut buf, ImageFormat::Png)
            .expect("encode test png");
        buf.into_inner()
    }

    #[test]
    fn test_flat_gray_image_is_detected_as_placeholder() {
        let gray = DynamicImage::ImageRgb8(RgbImage::from_pixel(240, 240, Rgb([128, 128, 128])));
        assert!(is_gray_placeholder(&gray));
    }

    #[test]
    fn test_monochrome_high_contrast_art_is_not_placeholder() {
        let mut img = RgbImage::new(240, 240);
        for (x, y, pixel) in img.enumerate_pixels_mut() {
            let luma = if (x / 30 + y / 30).is_multiple_of(2) {
                24
            } else {
                232
            };
            *pixel = Rgb([luma, luma, luma]);
        }
        let bw = DynamicImage::ImageRgb8(img);
        assert!(!is_gray_placeholder(&bw));

        let encoded = encode_png(bw.to_rgb8());
        assert!(decode_artwork(&encoded).is_ok());
    }

    #[test]
    fn test_color_image_is_not_placeholder() {
        let color = DynamicImage::ImageRgb8(RgbImage::from_pixel(240, 240, Rgb([200, 40, 40])));
        assert!(!is_gray_placeholder(&color));
    }
}
