use std::sync::LazyLock;

use image::{DynamicImage, GenericImageView};
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Style},
    widgets::Paragraph,
};

static DISCORD_ON: LazyLock<Option<DynamicImage>> = LazyLock::new(|| {
    let path = crate::paths::assets_dir().join("discord-on.png");
    image::open(path).ok().map(|img| crop_to_content(&img))
});

static DISCORD_OFF: LazyLock<Option<DynamicImage>> = LazyLock::new(|| {
    let path = crate::paths::assets_dir().join("discord-off.png");
    image::open(path).ok().map(|img| crop_to_content(&img))
});

fn crop_to_content(img: &DynamicImage) -> DynamicImage {
    let rgba = img.to_rgba8();
    let (w, h) = img.dimensions();
    let mut min_x = w;
    let mut min_y = h;
    let mut max_x = 0;
    let mut max_y = 0;
    let mut found = false;

    for y in 0..h {
        for x in 0..w {
            if rgba.get_pixel(x, y)[3] > 32 {
                found = true;
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
        }
    }

    if found && max_x >= min_x && max_y >= min_y {
        img.crop_imm(min_x, min_y, max_x - min_x + 1, max_y - min_y + 1)
    } else {
        img.clone()
    }
}

pub fn render_discord_indicator(frame: &mut Frame, area: Rect, connected: bool) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let (image_opt, id, blurple_or_gray) = if connected {
        (
            DISCORD_ON.as_ref(),
            "discord_indicator_on",
            Color::Rgb(88, 101, 242),
        )
    } else {
        (
            DISCORD_OFF.as_ref(),
            "discord_indicator_off",
            Color::Rgb(110, 110, 110),
        )
    };

    // If terminal supports high-resolution graphics, render smooth HQ image
    if let Some(img) = image_opt
        && crate::artwork::render_hq(frame, area, img, id)
    {
        return;
    }

    // Clean, non-pixelised compact indicator fallback
    let symbol = if connected { "●" } else { "○" };
    frame.render_widget(
        Paragraph::new(symbol)
            .style(Style::default().fg(blurple_or_gray))
            .alignment(Alignment::Center),
        area,
    );
}
