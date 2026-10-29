use std::{
    sync::{LazyLock, OnceLock},
    time::Instant,
};

use image::{DynamicImage, GenericImage, GenericImageView, Rgba, imageops::FilterType};
use ratatui::{
    Frame,
    layout::{Alignment, Rect, Size},
    style::{Color, Modifier, Style},
    widgets::Paragraph,
};
use ratatui_image::{Image as TerminalImage, Resize, picker::Picker};

use super::{cache::request_art, decode::HQ_SOURCE_SIDE};

static PICKER: OnceLock<Option<Picker>> = OnceLock::new();

pub fn init_picker() {
    let _ = PICKER.set(Picker::from_query_stdio().ok());
}

pub fn loading_moons() -> String {
    static START: LazyLock<Instant> = LazyLock::new(Instant::now);

    const FRAMES: [&str; 8] = ["🌑", "🌒", "🌓", "🌔", "🌕", "🌖", "🌗", "🌘"];
    const INTERVAL_MS: usize = 80;

    let elapsed = START.elapsed().as_millis() as usize;
    let index = elapsed / INTERVAL_MS;

    let moon = |phase: usize| FRAMES[phase % FRAMES.len()].to_string();
    format!("{} {} {}", moon(index), moon(index + 1), moon(index + 2))
}

fn picker() -> Option<&'static Picker> {
    PICKER.get().and_then(|picker| picker.as_ref())
}

pub fn render_art(
    frame: &mut Frame,
    area: Rect,
    url: &str,
    title: &str,
    artist: &str,
    hq: bool,
    accent: Option<Color>,
) {
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
                    let cropped =
                        image.crop_imm((width - crop) / 2, (height - crop) / 2, crop, crop);
                    let pixel_image = cropped.resize_exact(side, side, FilterType::Triangle);
                    render_pixels(frame, art_area, pixel_image, side);
                }
            }
            Err(message) if message == "loading" => {
                render_loading(frame, art_area, accent.unwrap_or(Color::Green))
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
        render_details(frame, area, title, artist, accent.unwrap_or(Color::Green));
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
    if side < 12 {
        return image;
    }

    const NOTE: [&str; 12] = [
        "....###.", "....####", "....##.#", "....##..", "....##..", "....##..", "....##..",
        "....##..", ".####...", "######..", "######..", ".####...",
    ];

    let scale = (side / 16).max(1);
    let note_w = 8 * scale;
    let note_h = 12 * scale;

    if side < note_w || side < note_h {
        return image;
    }

    let origin_x = (side - note_w) / 2;
    let origin_y = (side - note_h) / 2;
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

fn render_loading(frame: &mut Frame, area: Rect, accent: Color) {
    let accent_style = Style::default().fg(accent);
    let dim = Style::default().fg(Color::DarkGray);
    let moons = Rect {
        x: area.x,
        y: area.y + area.height / 2,
        width: area.width,
        height: 1,
    };

    frame.render_widget(
        Paragraph::new(loading_moons())
            .style(accent_style)
            .alignment(Alignment::Center),
        moons,
    );

    frame.render_widget(
        Paragraph::new("LOADING ART")
            .style(dim)
            .alignment(Alignment::Center),
        Rect {
            x: area.x,
            y: moons.y + 1,
            width: area.width,
            height: 1,
        },
    );
}

fn render_details(frame: &mut Frame, area: Rect, title: &str, artist: &str, accent: Color) {
    let rows = [
        (
            title,
            Style::default().fg(accent).add_modifier(Modifier::BOLD),
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
        text.chars().take(max.saturating_sub(1)).collect::<String>()
    )
}

fn render_pixels(frame: &mut Frame, area: Rect, image: DynamicImage, side: u32) {
    use ratatui::text::{Line, Span};

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
