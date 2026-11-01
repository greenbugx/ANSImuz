use serde::{Deserialize, Serialize};
use std::{env, fs, path::PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    #[default]
    #[serde(rename = "ytmusic")]
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

fn default_true() -> bool {
    true
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub audio_cache: bool,
    #[serde(default = "default_true")]
    pub art_hq: bool,
    #[serde(default)]
    pub art_palette: bool,
    #[serde(default)]
    pub provider: Provider,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            audio_cache: false,
            art_hq: true,
            art_palette: false,
            provider: Provider::YtMusic,
        }
    }
}

impl Settings {
    pub fn load() -> Self {
        let defaults = Self::default();

        let Ok(content) = fs::read_to_string(config_path()) else {
            return defaults;
        };

        // 1. Try parsing as TOML with Serde
        if let Ok(settings) = toml::from_str::<Settings>(&content) {
            return settings;
        }

        // 2. Legacy fallback for old key=value format
        let mut settings = defaults;
        for line in content.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };

            match key.trim() {
                "audio_cache" => settings.audio_cache = value.trim() == "true",
                "art_hq" => settings.art_hq = value.trim() == "true",
                "art_palette" => settings.art_palette = value.trim() == "true",
                "provider" => {
                    settings.provider = match value.trim().trim_matches('"').to_lowercase().as_str()
                    {
                        "spotify" => Provider::Spotify,
                        _ => Provider::YtMusic,
                    };
                }
                _ => {}
            }
        }
        settings
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = config_path();

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let toml_content = toml::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        fs::write(path, toml_content)
    }
}

pub fn config_path() -> PathBuf {
    let toml_path =
        env::var("HOME").map(|home| PathBuf::from(home).join(".config/ansimuz/settings.toml"));
    if let Ok(ref p) = toml_path
        && p.exists()
    {
        return p.clone();
    }

    let conf_path =
        env::var("HOME").map(|home| PathBuf::from(home).join(".config/ansimuz/settings.conf"));
    if let Ok(ref p) = conf_path
        && p.exists()
    {
        return p.clone();
    }

    toml_path.unwrap_or_else(|_| PathBuf::from(".ansimuz-settings.toml"))
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

pub fn clear_caches() -> std::io::Result<()> {
    let mut err = None;

    let art_dir = artwork_cache_dir();
    if art_dir.exists()
        && let Err(e) = fs::remove_dir_all(&art_dir)
    {
        err = Some(e);
    }

    let audio_dir = audio_cache_dir();
    if audio_dir.exists()
        && let Err(e) = fs::remove_dir_all(&audio_dir)
    {
        err = Some(e);
    }

    crate::artwork::clear_memory_cache();

    if let Some(e) = err { Err(e) } else { Ok(()) }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_settings_toml_serde_roundtrip() {
        let settings = Settings {
            audio_cache: true,
            art_hq: false,
            art_palette: true,
            provider: Provider::Spotify,
        };

        let toml_str = toml::to_string_pretty(&settings).expect("serialize toml");
        assert!(toml_str.contains("audio_cache = true"));
        assert!(toml_str.contains("art_hq = false"));
        assert!(toml_str.contains("art_palette = true"));
        assert!(toml_str.contains("provider = \"spotify\""));

        let loaded: Settings = toml::from_str(&toml_str).expect("deserialize toml");
        assert_eq!(loaded, settings);
    }

    #[test]
    fn test_settings_legacy_key_value_fallback() {
        let legacy = "audio_cache=true\nart_hq=false\nart_palette=true\nprovider=spotify\n";
        let parsed = toml::from_str::<Settings>(legacy);
        assert!(parsed.is_err(), "Legacy unquoted format is not valid TOML");

        let mut settings = Settings::default();
        for line in legacy.lines() {
            if let Some((key, value)) = line.split_once('=') {
                match key.trim() {
                    "audio_cache" => settings.audio_cache = value.trim() == "true",
                    "art_hq" => settings.art_hq = value.trim() == "true",
                    "art_palette" => settings.art_palette = value.trim() == "true",
                    "provider" => {
                        settings.provider =
                            match value.trim().trim_matches('"').to_lowercase().as_str() {
                                "spotify" => Provider::Spotify,
                                _ => Provider::YtMusic,
                            };
                    }
                    _ => {}
                }
            }
        }

        assert!(settings.audio_cache);
        assert!(!settings.art_hq);
        assert!(settings.art_palette);
        assert_eq!(settings.provider, Provider::Spotify);
    }

    #[test]
    fn test_clear_caches_result() {
        let result = clear_caches();
        assert!(result.is_ok(), "clear_caches must return Ok");
    }
}
