use std::sync::LazyLock;

use ratatui::text::Line;

static LOGO_BITMAP: LazyLock<Vec<Vec<bool>>> = LazyLock::new(init_logo_bitmap);

pub fn logo_bitmap() -> &'static [Vec<bool>] {
    &LOGO_BITMAP
}

fn init_logo_bitmap() -> Vec<Vec<bool>> {
    let lines: Vec<&str> = include_str!("../../../ascii.art")
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();

    let width = lines
        .iter()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(0);

    let mut bitmap = Vec::new();
    for line in &lines {
        let mut top = vec![false; width];
        let mut bottom = vec![false; width];
        for (x, ch) in line.chars().enumerate() {
            match ch {
                '▀' => top[x] = true,
                '▄' => bottom[x] = true,
                '█' | '▌' | '▐' => {
                    top[x] = true;
                    bottom[x] = true;
                }
                _ => {}
            }
        }
        bitmap.push(top);
        bitmap.push(bottom);
    }

    let right = (0..width)
        .rev()
        .find(|&x| bitmap.iter().any(|row| row.get(x) == Some(&true)))
        .map_or(0, |x| x + 1);
    for row in &mut bitmap {
        row.truncate(right);
    }

    bitmap
}

pub fn scale_bitmap(bitmap: &[Vec<bool>], width: usize, height: usize) -> Vec<Vec<bool>> {
    let src_height = bitmap.len();
    let src_width = bitmap.first().map_or(1, |row| row.len());

    (0..height)
        .map(|y| {
            let sy = y * src_height / height;
            (0..width)
                .map(|x| bitmap[sy][x * src_width / width])
                .collect()
        })
        .collect()
}

pub fn bitmap_lines(bitmap: &[Vec<bool>]) -> Vec<Line<'static>> {
    let width = bitmap.first().map_or(0, |row| row.len());

    bitmap
        .chunks(2)
        .map(|pair| {
            let line: String = (0..width)
                .map(
                    |x| match (pair[0][x], pair.get(1).is_some_and(|row| row[x])) {
                        (true, true) => '█',
                        (true, false) => '▀',
                        (false, true) => '▄',
                        (false, false) => ' ',
                    },
                )
                .collect();
            Line::from(line)
        })
        .collect()
}
