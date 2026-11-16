use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, Borders},
};

use crate::app::App;
use crate::ui::screens::player::{layout_player_panel, render_player_panel};
use crate::ui::theme::Theme;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LyricsLayout {
    pub col_lyrics: Rect,
    pub col_player: Rect,
}

pub fn layout_lyrics(area: Rect) -> LyricsLayout {
    let (player_w, lyrics_w) = if area.width >= 120 {
        let p_w = (area.width * 36 / 100).clamp(38, 52);
        (p_w, area.width.saturating_sub(p_w))
    } else if area.width >= 85 {
        let p_w = (area.width * 40 / 100).clamp(36, 46);
        (p_w, area.width.saturating_sub(p_w))
    } else {
        let p_w = (area.width / 2).max(20);
        (p_w, area.width.saturating_sub(p_w))
    };

    let cols = Layout::horizontal([Constraint::Length(lyrics_w), Constraint::Length(player_w)])
        .split(area);

    LyricsLayout {
        col_lyrics: cols[0],
        col_player: cols[1],
    }
}

pub fn render_lyrics(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    playback: Option<(f64, Option<f64>)>,
    theme: &Theme,
) {
    let layout = layout_lyrics(area);

    // Left / Center side: Lyrics panel (blank for now)
    let lyrics_block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.primary)
        .title(Line::from(vec![Span::styled(
            " LYRICS ",
            theme.bold_primary,
        )]));
    frame.render_widget(lyrics_block, layout.col_lyrics);

    // Right side: Player panel (without search bar)
    let panel_layout = layout_player_panel(layout.col_player, false);
    render_player_panel(
        frame,
        &panel_layout,
        layout.col_player,
        app,
        playback,
        theme,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    #[test]
    fn test_layout_lyrics_proportions_wide() {
        for width in [120, 140, 160, 200] {
            let area = Rect::new(0, 0, width, 30);
            let layout = layout_lyrics(area);

            assert!(layout.col_player.width >= 38 && layout.col_player.width <= 52);
            assert!(layout.col_lyrics.width >= 70);
            assert_eq!(
                layout.col_lyrics.width + layout.col_player.width,
                width,
                "Columns must sum to total width at {width}"
            );
            assert_eq!(layout.col_lyrics.x, 0);
            assert_eq!(layout.col_player.x, layout.col_lyrics.width);
        }
    }

    #[test]
    fn test_layout_lyrics_proportions_medium() {
        for width in [85, 90, 100, 110] {
            let area = Rect::new(0, 0, width, 30);
            let layout = layout_lyrics(area);

            assert!(layout.col_player.width >= 36 && layout.col_player.width <= 46);
            assert!(layout.col_lyrics.width >= 45);
            assert_eq!(
                layout.col_lyrics.width + layout.col_player.width,
                width,
                "Columns must sum to total width at {width}"
            );
            assert_eq!(layout.col_lyrics.x, 0);
            assert_eq!(layout.col_player.x, layout.col_lyrics.width);
        }
    }

    #[test]
    fn test_layout_lyrics_proportions_compact() {
        for width in [40, 50, 60, 70, 80] {
            let area = Rect::new(0, 0, width, 24);
            let layout = layout_lyrics(area);

            assert_eq!(
                layout.col_lyrics.width + layout.col_player.width,
                width,
                "Columns must sum to total width at {width}"
            );
            assert!(layout.col_player.width >= 20);
            assert!(layout.col_lyrics.width >= 20);
            assert_eq!(layout.col_lyrics.x, 0);
            assert_eq!(layout.col_player.x, layout.col_lyrics.width);
        }
    }

    #[test]
    fn test_render_lyrics_panel_structure() {
        let app = App::new();
        let theme = Theme::new(None);
        let area = Rect::new(0, 0, 120, 30);

        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                render_lyrics(frame, area, &app, None, &theme);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let layout = layout_lyrics(area);

        // Lyrics panel header on the left (left -> center)
        let lyrics_title_row = (layout.col_lyrics.x..layout.col_lyrics.x + layout.col_lyrics.width)
            .map(|x| buffer[(x, layout.col_lyrics.y)].symbol())
            .collect::<String>();
        assert!(
            lyrics_title_row.contains("LYRICS"),
            "Left column must contain LYRICS block, got: {lyrics_title_row}"
        );

        // Player panel header on the right
        let player_title_row = (layout.col_player.x..layout.col_player.x + layout.col_player.width)
            .map(|x| buffer[(x, layout.col_player.y)].symbol())
            .collect::<String>();
        assert!(
            player_title_row.contains("PLAYER"),
            "Right column must contain PLAYER block, got: {player_title_row}"
        );

        // Inner lyrics area is blank
        let inner_y = layout.col_lyrics.y + 1;
        let inner_row = (layout.col_lyrics.x + 1
            ..layout.col_lyrics.x + layout.col_lyrics.width - 1)
            .map(|x| buffer[(x, inner_y)].symbol())
            .collect::<String>();
        assert!(
            inner_row.trim().is_empty(),
            "Lyrics body must be blank for now, got: '{inner_row}'"
        );

        // Ensure search box is not rendered in the player panel
        let panel_layout = layout_player_panel(layout.col_player, false);
        assert!(
            !panel_layout.show_search,
            "Search must be disabled in lyrics screen"
        );
        let mut found_search_placeholder = false;
        for y in layout.col_player.y..layout.col_player.y + layout.col_player.height {
            let row = (layout.col_player.x..layout.col_player.x + layout.col_player.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>();
            if row.contains("Search songs") {
                found_search_placeholder = true;
                break;
            }
        }
        assert!(
            !found_search_placeholder,
            "Search bar must not be rendered in lyrics screen"
        );
    }
}
