use std::{env, fs, path::PathBuf};

use crate::{art, player};

#[derive(Clone, Copy, Debug)]
pub struct Settings {
    pub audio_cache: bool,
    pub art_hq: bool,
    pub art_palette: bool,
}

impl Settings {
    pub fn load() -> Self {
        let defaults = Self {
            audio_cache: false,
            art_hq: true,
            art_palette: false,
        };

        let Ok(content) = fs::read_to_string(config_path()) else {
            return defaults;
        };

        let mut settings = defaults;

        for line in content.lines() {
            let (key, value) = match line.split_once('=') {
                Some(pair) => pair,
                None => continue,
            };

            match key.trim() {
                "audio_cache" => settings.audio_cache = value.trim() == "true",
                "art_hq" => settings.art_hq = value.trim() == "true",
                "art_palette" => settings.art_palette = value.trim() == "true",
                _ => {}
            }
        }

        settings
    }

    pub fn save(&self) {
        let path = config_path();

        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        let _ = fs::write(
            path,
            format!(
                "audio_cache={}\nart_hq={}\nart_palette={}\n",
                self.audio_cache, self.art_hq, self.art_palette
            ),
        );
    }
}

fn config_path() -> PathBuf {
    env::var("HOME")
        .map(|home| PathBuf::from(home).join(".config/ansimuz/settings.conf"))
        .unwrap_or_else(|_| PathBuf::from(".ansimuz-settings"))
}

pub fn cache_bytes() -> u64 {
    dir_size(&art::cache_dir()) + dir_size(&player::audio_cache_dir())
}

pub fn clear_caches() {
    let _ = fs::remove_dir_all(art::cache_dir());
    let _ = fs::remove_dir_all(player::audio_cache_dir());
    art::clear_memory_cache();
}

pub fn human_size(bytes: u64) -> String {
    let kb = bytes as f64 / 1024.0;

    if kb < 1024.0 {
        format!("{kb:.1} KB")
    } else if kb < 1024.0 * 1024.0 {
        format!("{:.1} MB", kb / 1024.0)
    } else {
        format!("{:.1} GB", kb / 1024.0 / 1024.0)
    }
}

fn dir_size(path: &PathBuf) -> u64 {
    let Ok(entries) = fs::read_dir(path) else {
        return 0;
    };

    entries
        .filter_map(|entry| entry.ok())
        .map(|entry| {
            if entry.path().is_dir() {
                dir_size(&entry.path())
            } else {
                entry.metadata().map(|meta| meta.len()).unwrap_or(0)
            }
        })
        .sum()
}
