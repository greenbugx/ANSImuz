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
    let steps = 8u32;
    let dx = (w / steps).max(1);
    let dy = (h / steps).max(1);

    for sy in 0..steps {
        for sx in 0..steps {
            let x = (sx * dx).min(w - 1);
            let y = (sy * dy).min(h - 1);
            let [r, g, b, _] = image.get_pixel(x, y).0;
            if r != g || g != b {
                return false;
            }
        }
    }

    true
}
