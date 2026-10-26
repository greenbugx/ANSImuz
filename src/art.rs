use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
    sync::{Mutex, OnceLock},
    thread,
};

use image::{
    imageops::FilterType, DynamicImage, GenericImage, GenericImageView, Rgba,
};
use ratatui::{
    layout::{Rect, Size},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};
use ratatui_image::{picker::Picker, Image as TerminalImage, Resize};

const MAX_IMAGE_BYTES: usize = 8 * 1024 * 1024;
const HQ_SOURCE_SIDE: u32 = 0;

static PICKER: OnceLock<Option<Picker>> = OnceLock::new();

pub fn init_picker() {
    let _ = PICKER.set(Picker::from_query_stdio().ok());
}

fn picker() -> Option<&'static Picker> {
    PICKER.get().and_then(|picker| picker.as_ref())
}

fn cache_dir() -> PathBuf {
    std::env::temp_dir().join("ansimuz-artwork")
}

fn cache_path(url: &str) -> PathBuf {
    let hash = url.bytes().fold(2166136261u32, |hash, byte| {
        (hash ^ u32::from(byte)).wrapping_mul(16777619)
    });

    cache_dir().join(format!("{hash:08x}.img"))
}

fn load_image(url: &str) -> Result<DynamicImage, String> {
    if url.trim().is_empty() {
        return Err("No artwork URL".into());
    }

    let path = cache_path(url);

    if path.exists() {
        if let Ok(bytes) = fs::read(&path) {
            if let Ok(image) = image::load_from_memory(&bytes) {
                return Ok(image);
            }
        }
        let _ = fs::remove_file(&path);
    }

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .user_agent("ANSIMUZ/0.1")
        .build()
        .map_err(|e| e.to_string())?;

    let mut response = client
        .get(url)
        .send()
        .map_err(|e| format!("Artwork download failed: {e}"))?;

    if !response.status().is_success() {
        if let Some(fallback_url) = thumbnail_fallback(url) {
            response = client
                .get(&fallback_url)
                .send()
                .map_err(|e| format!("Artwork download failed: {e}"))?;
        }
    }

    let response = response
        .error_for_status()
        .map_err(|e| format!("Artwork HTTP error: {e}"))?;

    if response
        .content_length()
        .is_some_and(|size| size > MAX_IMAGE_BYTES as u64)
    {
        return Err("Artwork image is too large".into());
    }

    let bytes = response
        .bytes()
        .map_err(|e| format!("Cannot read artwork: {e}"))?;

    if bytes.len() > MAX_IMAGE_BYTES {
        return Err("Artwork image is too large".into());
    }

    let image = image::load_from_memory(&bytes)
        .map_err(|e| format!("Cannot decode artwork: {e}"))?;

    let (width, height) = image.dimensions();
    let image = if width > height {
        let left = (width - height) / 2;
        image.crop_imm(left, 0, height, height)
    } else {
        image
    };

    let _ = fs::create_dir_all(cache_dir());
    let _ = fs::write(path, &bytes);

    Ok(image)
}

fn thumbnail_fallback(url: &str) -> Option<String> {
    (url.contains("img.youtube.com") && url.contains("maxresdefault"))
        .then(|| url.replace("maxresdefault", "hqdefault"))
}

fn art_cache() -> &'static Mutex<HashMap<(String, u32), Result<DynamicImage, String>>> {
    static CACHE: OnceLock<Mutex<HashMap<(String, u32), Result<DynamicImage, String>>>> =
        OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn request_art(url: &str, side: u32) -> Result<DynamicImage, String> {
    let key = (url.to_string(), side);

    if let Ok(guard) = art_cache().lock() {
        if let Some(image) = guard.get(&key) {
            return image.clone();
        }
    }

    if let Ok(mut guard) = art_cache().lock() {
        guard.insert(key.clone(), Err("loading".into()));
    }

    let owned_url = url.to_string();

    thread::spawn(move || {
        let result = load_image(&owned_url).map(|image| {
            if side == 0 {
                image
            } else {
                image.resize_exact(side, side, FilterType::Triangle)
            }
        });

        if let Ok(mut guard) = art_cache().lock() {
            guard.insert(key, result);
        }
    });

    Err("loading".into())
}

pub fn prefetch(url: &str) {
    if url.trim().is_empty() {
        return;
    }

    let _ = request_art(url, 64);
}

pub fn render_art(frame: &mut Frame, area: Rect, url: &str, title: &str, artist: &str, hq: bool) {
    if area.width < 4 || area.height < 3 {
        return;
    }

    let art_area = Rect {
        x: area.x,
        y: area.y,
        width: area.width,
        height: area.height.saturating_sub(3),
    };

    let side = ((art_area.height as u32) * 2)
        .min(area.width as u32 * 2 / 5)
        .max(2)
        & !1;

    let hq_rows = art_area.height;
    let hq_cols = (hq_rows * 2).min(art_area.width);
    let hq_area = Rect {
        x: art_area.x + (art_area.width - hq_cols) / 2,
        y: art_area.y,
        width: hq_cols,
        height: hq_rows,
    };

    if url.is_empty() {
        let fallback = fallback_image(side);

        if !(hq && render_hq(frame, hq_area, &fallback)) {
            render_pixels(frame, art_area, fallback, side);
        }
    } else {
        match request_art(url, HQ_SOURCE_SIDE) {
            Ok(image) => {
                if !(hq && render_hq(frame, hq_area, &image)) {
                    let (width, height) = image.dimensions();
                    let crop = width.min(height);
                    let cropped = image.crop_imm(
                        (width - crop) / 2,
                        (height - crop) / 2,
                        crop,
                        crop,
                    );
                    let pixel_image =
                        cropped.resize_exact(side, side, FilterType::Triangle);
                    render_pixels(frame, art_area, pixel_image, side);
                }
            }
            Err(message) if message == "loading" => {
                render_message(frame, art_area, "LOADING ART…", Color::DarkGray)
            }
            Err(_) => {
                let fallback = fallback_image(side);

                if !(hq && render_hq(frame, hq_area, &fallback)) {
                    render_pixels(frame, art_area, fallback, side);
                }
            }
        }
    }

    if area.height > 3 {
        render_details(frame, area, title, artist);
    }
}

fn render_hq(frame: &mut Frame, area: Rect, image: &DynamicImage) -> bool {
    let Some(picker) = picker() else {
        return false;
    };

    let font_size = picker.font_size();
    let target_w = (area.width as u32 * u32::from(font_size.width)).max(1);
    let target_h = (area.height as u32 * u32::from(font_size.height)).max(1);

    let scaled = image.resize(target_w, target_h, FilterType::Triangle);

    let Ok(protocol) = picker.new_protocol(
        scaled,
        Size::new(area.width, area.height),
        Resize::Fit(Some(FilterType::Triangle)),
    ) else {
        return false;
    };

    frame.render_widget(TerminalImage::new(&protocol).allow_clipping(true), area);

    true
}

pub fn dominant_color(url: &str) -> Option<(u8, u8, u8)> {
    let guard = art_cache().lock().ok()?;

    for (key, entry) in guard.iter() {
        if key.0 != url {
            continue;
        }

        if let Ok(image) = entry {
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

fn brighten(color: (u8, u8, u8)) -> (u8, u8, u8) {
    let (r, g, b) = color;
    let luminance = 0.299 * f32::from(r) + 0.587 * f32::from(g) + 0.114 * f32::from(b);

    if luminance >= 90.0 {
        return color;
    }

    let scale = 90.0 / luminance.max(1.0);
    let channel = |value: u8| (f32::from(value) * scale).min(255.0) as u8;
    (channel(r), channel(g), channel(b))
}

fn fallback_image(side: u32) -> DynamicImage {
    let mut image = DynamicImage::new_rgb8(side, side);

    for y in 0..side {
        let shade = (26.0 + 34.0 * (y as f32 / side.max(1) as f32)) as u8;

        for x in 0..side {
            let pixel = if (x + y) % 12 < 3 {
                Rgba([16, shade + 8, 20, 255])
            } else {
                Rgba([12, shade, 16, 255])
            };

            image.put_pixel(x, y, pixel);
        }
    }

    const NOTE: [&str; 12] = [
        "....###.",
        "....####",
        "....##.#",
        "....##..",
        "....##..",
        "....##..",
        "....##..",
        "....##..",
        ".####...",
        "######..",
        "######..",
        ".####...",
    ];

    let scale = (side / 16).max(2);
    let origin_x = (side - 8 * scale) / 2;
    let origin_y = (side - 12 * scale) / 2;
    let note = Rgba([130, 255, 150, 255]);

    for (row, pattern) in NOTE.iter().enumerate() {
        for (col, ch) in pattern.chars().enumerate() {
            if ch != '#' {
                continue;
            }

            for dy in 0..scale {
                for dx in 0..scale {
                    image.put_pixel(
                        origin_x + col as u32 * scale + dx,
                        origin_y + row as u32 * scale + dy,
                        note,
                    );
                }
            }
        }
    }

    image
}

fn render_message(frame: &mut Frame, area: Rect, message: &str, color: Color) {
    let y = area.y + area.height / 2;
    frame.render_widget(
        Paragraph::new(message)
            .style(Style::default().fg(color))
            .centered(),
        Rect {
            x: area.x,
            y,
            width: area.width,
            height: 1,
        },
    );
}

fn render_details(frame: &mut Frame, area: Rect, title: &str, artist: &str) {
    let rows = [
        (
            title,
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        (artist, Style::default().fg(Color::DarkGray)),
    ];

    for (offset, (text, style)) in rows.iter().enumerate() {
        if text.is_empty() {
            continue;
        }

        frame.render_widget(
            Paragraph::new(fit(text, area.width as usize))
                .style(*style)
                .centered(),
            Rect {
                x: area.x,
                y: area.y + area.height - 2 + offset as u16,
                width: area.width,
                height: 1,
            },
        );
    }
}

fn fit(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }

    format!(
        "{}…",
        text.chars()
            .take(max.saturating_sub(1))
            .collect::<String>()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const PIXEL_PNG: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90,
        0x77, 0x53, 0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, 0x08, 0xD7, 0x63, 0xF8,
        0xCF, 0xC0, 0x00, 0x00, 0x03, 0x01, 0x01, 0x00, 0x18, 0xDD, 0x8D, 0xB0, 0x00, 0x00, 0x00,
        0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];

    #[test]
    fn cached_artwork_survives_extensionless_disk_cache() {
        let dir = std::env::temp_dir().join(format!("ansimuz-test-{}", std::process::id()));
        fs::create_dir_all(&dir).expect("cache dir");

        let path = dir.join("cache.img");
        fs::write(&path, PIXEL_PNG).expect("write cache file");

        let legacy = image::open(&path);
        assert!(legacy.is_err(), "image::open cannot detect .img files");

        let bytes = fs::read(&path).expect("read cache file");
        let image = image::load_from_memory(&bytes).expect("content-based load");
        assert_eq!(image.dimensions(), (1, 1));

        fs::remove_dir_all(&dir).expect("cleanup");
    }

    #[test]
    fn corrupt_cache_is_reported_not_panicked() {
        let broken = image::load_from_memory(b"not an image");
        assert!(broken.is_err());
    }
}

fn render_pixels(frame: &mut Frame, area: Rect, image: DynamicImage, side: u32) {
    let cols = side as u16;
    let rows = (side / 2) as u16;

    if cols > area.width || rows > area.height {
        return;
    }

    let rect = Rect {
        x: area.x + (area.width - cols) / 2,
        y: area.y + (area.height - rows) / 2,
        width: cols,
        height: rows,
    };

    let mut lines = Vec::new();

    for y in (0..side).step_by(2) {
        let mut spans = Vec::new();

        for x in 0..side {
            let top = image.get_pixel(x, y).0;
            let bottom = image.get_pixel(x, (y + 1).min(side - 1)).0;

            spans.push(Span::styled(
                "▀",
                Style::default()
                    .fg(Color::Rgb(top[0], top[1], top[2]))
                    .bg(Color::Rgb(bottom[0], bottom[1], bottom[2])),
            ));
        }

        lines.push(Line::from(spans));
    }

    frame.render_widget(Paragraph::new(lines), rect);
}
