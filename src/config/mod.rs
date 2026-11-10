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
    #[serde(default)]
    pub discord_rpc: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            audio_cache: false,
            art_hq: true,
            art_palette: false,
            provider: Provider::YtMusic,
            discord_rpc: false,
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
                "discord_rpc" => settings.discord_rpc = value.trim() == "true",
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

static CACHE_TEMP_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static CACHE_EPOCH: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub const MAX_CACHE_DIR_BYTES: u64 = 512 * 1024 * 1024; // 512 MB per cache type

pub fn current_cache_epoch() -> u64 {
    CACHE_EPOCH.load(std::sync::atomic::Ordering::Relaxed)
}

pub fn invalidate_cache_epoch() -> u64 {
    CACHE_EPOCH.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1
}

pub fn clean_stale_part_files(dir: &std::path::Path) -> std::io::Result<usize> {
    if !dir.exists() {
        return Ok(0);
    }
    let mut cleaned = 0;
    let entries = fs::read_dir(dir)?;
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_file()
            && path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext == "part")
                .unwrap_or(false)
            && fs::remove_file(&path).is_ok()
        {
            cleaned += 1;
        }
    }
    Ok(cleaned)
}

pub fn clean_stale_caches() {
    let _ = clean_stale_part_files(&artwork_cache_dir());
    let _ = clean_stale_part_files(&audio_cache_dir());
}

pub fn prune_cache_dir(dir: &std::path::Path, max_bytes: u64) -> std::io::Result<u64> {
    if !dir.exists() {
        return Ok(0);
    }

    let mut entries = Vec::new();
    let mut total_size = 0u64;

    for entry in fs::read_dir(dir)?.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_file()
            && let Ok(meta) = entry.metadata()
        {
            let size = meta.len();
            total_size += size;
            let mtime = meta.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            entries.push((path, size, mtime));
        }
    }

    if total_size <= max_bytes {
        return Ok(0);
    }

    // Sort oldest modified first
    entries.sort_by_key(|(_, _, mtime)| *mtime);

    let mut bytes_pruned = 0u64;
    for (path, size, _) in entries {
        if total_size <= max_bytes {
            break;
        }
        if fs::remove_file(&path).is_ok() {
            total_size = total_size.saturating_sub(size);
            bytes_pruned += size;
        }
    }

    Ok(bytes_pruned)
}

pub fn write_cache_atomic(
    dir: &std::path::Path,
    file_stem: &str,
    extension: &str,
    bytes: &[u8],
) -> std::io::Result<PathBuf> {
    fs::create_dir_all(dir)?;
    let final_path = dir.join(format!("{file_stem}.{extension}"));
    let temp_id = CACHE_TEMP_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let temp_path = dir.join(format!(
        "{file_stem}.tmp-{}-{temp_id}.part",
        std::process::id()
    ));

    if let Err(e) = fs::write(&temp_path, bytes) {
        let _ = fs::remove_file(&temp_path);
        return Err(e);
    }

    if let Err(e) = fs::rename(&temp_path, &final_path) {
        // Handle cross-platform atomic replacement
        if e.kind() == std::io::ErrorKind::AlreadyExists
            || e.kind() == std::io::ErrorKind::PermissionDenied
        {
            let _ = fs::remove_file(&final_path);
            if let Err(retry_err) = fs::rename(&temp_path, &final_path) {
                let _ = fs::remove_file(&temp_path);
                return Err(retry_err);
            }
        } else {
            let _ = fs::remove_file(&temp_path);
            return Err(e);
        }
    }

    let _ = prune_cache_dir(dir, MAX_CACHE_DIR_BYTES);

    Ok(final_path)
}

pub fn clear_caches() -> std::io::Result<()> {
    invalidate_cache_epoch();

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
    crate::music::clear_resolved_cache();
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
            discord_rpc: true,
        };

        let toml_str = toml::to_string_pretty(&settings).expect("serialize toml");
        assert!(toml_str.contains("audio_cache = true"));
        assert!(toml_str.contains("art_hq = false"));
        assert!(toml_str.contains("art_palette = true"));
        assert!(toml_str.contains("provider = \"spotify\""));
        assert!(toml_str.contains("discord_rpc = true"));

        let loaded: Settings = toml::from_str(&toml_str).expect("deserialize toml");
        assert_eq!(loaded, settings);
    }

    #[test]
    fn test_settings_missing_discord_rpc_defaults_to_false() {
        let existing_toml =
            "audio_cache = true\nart_hq = true\nart_palette = false\nprovider = \"ytmusic\"\n";
        let loaded: Settings = toml::from_str(existing_toml).expect("deserialize existing toml");
        assert!(
            !loaded.discord_rpc,
            "Existing config without discord_rpc must default to false"
        );
    }

    #[test]
    fn test_settings_legacy_key_value_fallback() {
        let legacy = "audio_cache=true\nart_hq=false\nart_palette=true\nprovider=spotify\ndiscord_rpc=true\n";
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
                    "discord_rpc" => settings.discord_rpc = value.trim() == "true",
                    _ => {}
                }
            }
        }

        assert!(settings.audio_cache);
        assert!(!settings.art_hq);
        assert!(settings.art_palette);
        assert_eq!(settings.provider, Provider::Spotify);
        assert!(settings.discord_rpc);
    }

    #[test]
    fn test_clear_caches_result() {
        let result = clear_caches();
        assert!(result.is_ok(), "clear_caches must return Ok");
    }
    #[test]
    fn test_write_cache_atomic_overwrites_existing_file() {
        let dir = std::env::temp_dir().join(format!("ansimuz-test-atomic-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);

        let path1 = write_cache_atomic(&dir, "sample", "txt", b"version 1").expect("write 1");
        assert_eq!(fs::read(&path1).unwrap(), b"version 1");

        // Overwrite must succeed on all platforms
        let path2 = write_cache_atomic(&dir, "sample", "txt", b"version 2").expect("write 2");
        assert_eq!(path1, path2);
        assert_eq!(fs::read(&path2).unwrap(), b"version 2");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_clean_stale_part_files() {
        let dir = std::env::temp_dir().join(format!("ansimuz-test-stale-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);

        fs::write(dir.join("track1.part"), b"stale data 1").unwrap();
        fs::write(dir.join("track2.part"), b"stale data 2").unwrap();
        fs::write(dir.join("valid.txt"), b"keeper").unwrap();

        let cleaned = clean_stale_part_files(&dir).expect("clean stale");
        assert_eq!(cleaned, 2);
        assert!(!dir.join("track1.part").exists());
        assert!(!dir.join("track2.part").exists());
        assert!(dir.join("valid.txt").exists());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_prune_cache_dir_enforces_max_bytes() {
        let dir = std::env::temp_dir().join(format!("ansimuz-test-prune-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);

        fs::write(dir.join("file1.dat"), vec![0u8; 1000]).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(50));
        fs::write(dir.join("file2.dat"), vec![0u8; 1000]).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(50));
        fs::write(dir.join("file3.dat"), vec![0u8; 1000]).unwrap();

        assert_eq!(dir_size(&dir), 3000);

        // Prune down to 1500 bytes
        let pruned = prune_cache_dir(&dir, 1500).expect("prune");
        assert_eq!(pruned, 2000);
        assert_eq!(dir_size(&dir), 1000);
        assert!(dir.join("file3.dat").exists());
        assert!(!dir.join("file1.dat").exists());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_cache_epoch_invalidation() {
        let epoch1 = current_cache_epoch();
        let epoch2 = invalidate_cache_epoch();
        assert_eq!(epoch2, epoch1 + 1);
        assert_eq!(current_cache_epoch(), epoch2);
    }
}
