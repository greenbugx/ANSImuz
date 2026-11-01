use image::GenericImageView;

use super::cache::{CacheEntry, art_cache};

pub fn dominant_color(url: &str) -> Option<(u8, u8, u8)> {
    let guard = art_cache().lock().ok()?;

    for (key, entry) in guard.iter() {
        if key.0 != url {
            continue;
        }

        if let CacheEntry::Ready(image) = entry {
            let (width, height) = image.dimensions();
            let (mut sum_r, mut sum_g, mut sum_b, mut count) = (0u64, 0u64, 0u64, 0u64);

            for y in (0..height).step_by(2) {
                for x in (0..width).step_by(2) {
                    let pixel = image.get_pixel(x, y).0;
                    sum_r += u64::from(pixel[0]);
                    sum_g += u64::from(pixel[1]);
                    sum_b += u64::from(pixel[2]);
                    count += 1;
                }
            }

            if count == 0 {
                continue;
            }

            return Some(brighten((
                (sum_r / count) as u8,
                (sum_g / count) as u8,
                (sum_b / count) as u8,
            )));
        }
    }

    None
}

pub fn brighten(color: (u8, u8, u8)) -> (u8, u8, u8) {
    let (r, g, b) = color;
    let luminance = 0.299 * f32::from(r) + 0.587 * f32::from(g) + 0.114 * f32::from(b);

    if luminance >= 90.0 {
        return color;
    }

    let scale = 90.0 / luminance.max(1.0);
    let channel = |value: u8| (f32::from(value) * scale).min(255.0) as u8;
    (channel(r), channel(g), channel(b))
}
