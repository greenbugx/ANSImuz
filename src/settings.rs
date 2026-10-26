use std::{env, fs, path::PathBuf};

#[derive(Clone, Copy, Debug)]
pub struct Settings {
    pub audio_cache: bool,
}

impl Settings {
    pub fn load() -> Self {
        let defaults = Self { audio_cache: false };

        let Ok(content) = fs::read_to_string(config_path()) else {
            return defaults;
        };

        let mut settings = defaults;
        for line in content.lines() {
            if let Some(value) = line.strip_prefix("audio_cache=") {
                settings.audio_cache = value.trim() == "true";
            }
        }
        settings
    }

    pub fn save(&self) {
        let path = config_path();

        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(path, format!("audio_cache={}\n", self.audio_cache));
    }
}

fn config_path() -> PathBuf {
    env::var("HOME")
        .map(|home| PathBuf::from(home).join(".config/ansimuz/settings.conf"))
        .unwrap_or_else(|_| PathBuf::from(".ansimuz-settings"))
}
