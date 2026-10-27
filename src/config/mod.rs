use std::{env, fs, path::PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Provider {
    YtMusic,
    Spotify,
}

impl Provider {
    pub fn label(self) -> &'static str {
        match self {
            Provider::YtMusic => "YOUTUBE MUSIC",
            Provider::Spotify => "SPOTIFY",
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Provider::YtMusic => "ytmusic",
            Provider::Spotify => "spotify",
        }
    }

    pub fn toggle(self) -> Self {
        match self {
            Provider::YtMusic => Provider::Spotify,
            Provider::Spotify => Provider::YtMusic,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Settings {
    pub audio_cache: bool,
    pub art_hq: bool,
    pub art_palette: bool,
    pub provider: Provider,
}

impl Settings {
    pub fn load() -> Self {
        let defaults = Self {
            audio_cache: false,
            art_hq: true,
            art_palette: false,
            provider: Provider::YtMusic,
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
                "provider" => {
                    settings.provider = match value.trim().to_lowercase().as_str() {
                        "spotify" => Provider::Spotify,
                        _ => Provider::YtMusic,
                    };
                }
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
                "audio_cache={}\nart_hq={}\nart_palette={}\nprovider={}\n",
                self.audio_cache,
                self.art_hq,
                self.art_palette,
                self.provider.as_str()
            ),
        );
    }
}

pub fn config_path() -> PathBuf {
    env::var("HOME")
        .map(|home| PathBuf::from(home).join(".config/ansimuz/settings.conf"))
        .unwrap_or_else(|_| PathBuf::from(".ansimuz-settings"))
}

pub fn cache_base_dir() -> PathBuf {
    env::var("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            env::var("HOME")
                .map(|h| PathBuf::from(h).join(".cache"))
                .unwrap_or_else(|_| env::temp_dir())
        })
        .join("ansimuz")
}

pub fn artwork_cache_dir() -> PathBuf {
    cache_base_dir().join("artwork")
}

pub fn audio_cache_dir() -> PathBuf {
    cache_base_dir().join("audio")
}

pub fn cache_bytes() -> u64 {
    dir_size(&artwork_cache_dir()) + dir_size(&audio_cache_dir())
}

pub fn clear_caches() {
    let _ = fs::remove_dir_all(artwork_cache_dir());
    let _ = fs::remove_dir_all(audio_cache_dir());
    crate::artwork::clear_memory_cache();
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

pub fn dir_size(path: &PathBuf) -> u64 {
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
