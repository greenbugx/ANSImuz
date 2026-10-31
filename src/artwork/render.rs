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

    let has_details = !title.is_empty() || !artist.is_empty();
    let details_reserved: u16 = if has_details && area.height > 6 { 3 } else { 0 };

    let art_area = Rect {
        x: area.x,
        y: area.y,
        width: area.width,
        height: area.height.saturating_sub(details_reserved),
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

    let render_fallback = |frame: &mut Frame| {
        if let Some(frame_img) = super::animation::current_animation_frame() {
            render_animated_frame(frame, art_area, &frame_img);
        } else {
            let fallback = fallback_image(side);
            if !(hq && render_hq(frame, hq_area, &fallback)) {
                render_pixels(frame, art_area, fallback, side);
            }
        }
    };

    if url.is_empty() {
        render_fallback(frame);
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
                render_fallback(frame);
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

pub fn render_static_image(frame: &mut Frame, area: Rect, image: &DynamicImage) {
    if area.width < 2 || area.height < 1 {
        return;
    }

    if render_hq(frame, area, image) {
        return;
    }

    let side = ((area.height as u32) * 2).min(area.width as u32).max(2) & !1;
    let (w, h) = image.dimensions();
    if w == 0 || h == 0 {
        return;
    }

    let target_h = (side * h / w.max(1)).max(1);
    let resized = image.resize_exact(side, target_h, FilterType::Triangle);

    let mut square = DynamicImage::new_rgba8(side, side);
    let y_offset = (side.saturating_sub(target_h)) / 2;
    image::imageops::overlay(&mut square, &resized, 0, y_offset as i64);

    render_pixels(frame, area, square, side);
}

fn render_animated_frame(frame: &mut Frame, art_area: Rect, image: &DynamicImage) {
    // Make animation smaller (~68% of art_area height, at least 4 rows)
    let anim_rows = if art_area.height >= 12 {
        (art_area.height * 68 / 100).max(4)
    } else if art_area.height >= 8 {
        art_area.height.saturating_sub(2).max(4)
    } else {
        art_area.height
    };

    let anim_cols = (anim_rows * 2).min(art_area.width);

    // Calculate vertical slack and position the animation lower down
    let slack = art_area.height.saturating_sub(anim_rows);
    let y_shift = if slack >= 2 {
        slack.saturating_sub(1)
    } else {
        slack
    };

    let hq_area = Rect {
        x: art_area.x + (art_area.width - anim_cols) / 2,
        y: art_area.y + y_shift,
        width: anim_cols,
        height: anim_rows,
    };

    // Animation is HD only: always use high-definition terminal image protocol
    // if supported by the terminal, unaffected by the art_hq setting.
    if render_hq(frame, hq_area, image) {
        return;
    }

    // Fallback for terminals without graphics protocol support
    let side = (anim_rows as u32 * 2)
        .min(art_area.width as u32 * 2 / 5)
        .max(2)
        & !1;

    let (w, h) = image.dimensions();
    let target_h = (side * h / w.max(1)).max(1);
    let resized = image.resize_exact(side, target_h, FilterType::Triangle);

    let mut square = DynamicImage::new_rgba8(side, side);
    let y_offset = (side.saturating_sub(target_h)) / 2;
    image::imageops::overlay(&mut square, &resized, 0, y_offset as i64);

    let adjusted_art_area = Rect {
        x: art_area.x,
        y: art_area.y + y_shift,
        width: art_area.width,
        height: art_area.height.saturating_sub(y_shift),
    };

    render_pixels(frame, adjusted_art_area, square, side);
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

    let y_slack = area.height.saturating_sub(rows);
    let y_center = y_slack / 2;
    let y_extra = if y_slack >= 4 {
        2
    } else if y_slack >= 2 {
        1
    } else {
        0
    };

    let rect = Rect {
        x: area.x + (area.width - cols) / 2,
        y: area.y + y_center + y_extra,
        width: cols,
        height: rows,
    };
    let mut lines = Vec::new();

    for y in (0..side).step_by(2) {
        let mut spans = Vec::new();

        for x in 0..side {
            let top = image.get_pixel(x, y).0;
            let bottom = image.get_pixel(x, (y + 1).min(side - 1)).0;

            let top_visible = top[3] >= 128;
            let bottom_visible = bottom[3] >= 128;

            let span = match (top_visible, bottom_visible) {
                (true, true) => Span::styled(
                    "▀",
                    Style::default()
                        .fg(Color::Rgb(top[0], top[1], top[2]))
                        .bg(Color::Rgb(bottom[0], bottom[1], bottom[2])),
                ),
                (true, false) => {
                    Span::styled("▀", Style::default().fg(Color::Rgb(top[0], top[1], top[2])))
                }
                (false, true) => Span::styled(
                    "▄",
                    Style::default().fg(Color::Rgb(bottom[0], bottom[1], bottom[2])),
                ),
                (false, false) => Span::raw(" "),
            };

            spans.push(span);
        }

        lines.push(Line::from(spans));
    }

    frame.render_widget(Paragraph::new(lines), rect);
}
