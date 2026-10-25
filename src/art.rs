use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
    sync::{Mutex, OnceLock},
};

use image::{imageops::FilterType, DynamicImage, GenericImageView};
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

const MAX_IMAGE_BYTES: usize = 8 * 1024 * 1024;
const MAX_ART_SIDE: u32 = 64;

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
        if let Ok(image) = image::open(&path) {
            return Ok(image);
        }
        let _ = fs::remove_file(&path);
    }

    let response = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .user_agent("ANSIMUZ/0.1")
        .build()
        .map_err(|e| e.to_string())?
        .get(url)
        .send()
        .map_err(|e| format!("Artwork download failed: {e}"))?
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

    let _ = fs::create_dir_all(cache_dir());
    let _ = fs::write(path, &bytes);

    Ok(image)
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

    std::thread::spawn(move || {
        let result = load_image(&owned_url)
            .map(|image| image.resize_exact(side, side, FilterType::Triangle));

        if let Ok(mut guard) = art_cache().lock() {
            guard.insert(key, result);
        }
    });

    Err("loading".into())
}

pub fn render_art(frame: &mut Frame, area: Rect, url: &str, title: &str, artist: &str) {
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
        .min(MAX_ART_SIDE)
        .max(2)
        & !1;

    if url.is_empty() {
        render_message(frame, art_area, "NO ARTWORK", Color::DarkGray);
    } else {
        match request_art(url, side) {
            Ok(image) => render_pixels(frame, art_area, image, side),
            Err(message) if message == "loading" => {
                render_message(frame, art_area, "LOADING ART…", Color::DarkGray)
            }
            Err(_) => render_message(frame, art_area, "ART UNAVAILABLE", Color::DarkGray),
        }
    }

    if area.height > 3 {
        render_details(frame, area, title, artist);
    }
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
