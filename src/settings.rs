use std::{env, fs, path::PathBuf};

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
