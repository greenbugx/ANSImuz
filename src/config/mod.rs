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

fn default_cache_max_size_mb() -> u64 {
    250
}

fn default_max_download_size_mb() -> u64 {
    8
}

fn default_image_size() -> u32 {
    1024
}

fn default_primary_host() -> String {
    "imgdb".to_string()
}

fn default_fallback_host() -> String {
    "catbox".to_string()
}

/// Supported Litterbox temporary-hosting retention durations.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum LitterboxExpiry {
    #[serde(rename = "1h")]
    OneHour,
    #[serde(rename = "12h")]
    TwelveHours,
    #[serde(rename = "24h")]
    TwentyFourHours,
    #[default]
    #[serde(rename = "72h")]
    SeventyTwoHours,
}

impl LitterboxExpiry {
    /// API time form parameter value sent to Litterbox.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OneHour => "1h",
            Self::TwelveHours => "12h",
            Self::TwentyFourHours => "24h",
            Self::SeventyTwoHours => "72h",
        }
    }

    /// Exact retention duration in seconds matching [`Self::as_str`].
    #[must_use]
    pub const fn duration_seconds(self) -> u64 {
        match self {
            Self::OneHour => 3600,
            Self::TwelveHours => 12 * 3600,
            Self::TwentyFourHours => 24 * 3600,
            Self::SeventyTwoHours => 72 * 3600,
        }
    }
}

/// Artwork-specific configuration for Discord Rich Presence.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscordArtworkConfig {
    #[serde(default = "default_cache_max_size_mb")]
    pub cache_max_size_mb: u64,
    #[serde(default = "default_max_download_size_mb")]
    pub max_download_size_mb: u64,
    #[serde(default = "default_image_size")]
    pub image_size: u32,
    #[serde(default = "default_primary_host")]
    pub primary_host: String,
    #[serde(default = "default_fallback_host")]
    pub fallback_host: String,
    #[serde(default)]
    pub litterbox_expiry: LitterboxExpiry,
}

impl Default for DiscordArtworkConfig {
    fn default() -> Self {
        Self {
            cache_max_size_mb: default_cache_max_size_mb(),
            max_download_size_mb: default_max_download_size_mb(),
            image_size: default_image_size(),
            primary_host: default_primary_host(),
            fallback_host: default_fallback_host(),
            litterbox_expiry: LitterboxExpiry::default(),
        }
    }
}

impl DiscordArtworkConfig {
    #[must_use]
    pub fn cache_max_bytes(&self) -> u64 {
        self.cache_max_size_mb.saturating_mul(1024 * 1024)
    }

    #[must_use]
    pub fn max_download_bytes(&self) -> usize {
        (self.max_download_size_mb.saturating_mul(1024 * 1024)) as usize
    }

    #[must_use]
    pub const fn litterbox_duration_seconds(&self) -> u64 {
        self.litterbox_expiry.duration_seconds()
    }

    #[must_use]
    pub const fn litterbox_time_str(&self) -> &'static str {
        self.litterbox_expiry.as_str()
    }
}

/// Discord Rich Presence subsystem configuration.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscordConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub artwork_enabled: bool,
    #[serde(default)]
    pub artwork: DiscordArtworkConfig,
}

impl Default for DiscordConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            artwork_enabled: true,
            artwork: DiscordArtworkConfig::default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
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
    pub discord: DiscordConfig,
}

#[derive(Deserialize)]
struct DiscordConfigDeser {
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default = "default_true")]
    artwork_enabled: bool,
    #[serde(default)]
    artwork: DiscordArtworkConfig,
}

impl Default for DiscordConfigDeser {
    fn default() -> Self {
        Self {
            enabled: None,
            artwork_enabled: true,
            artwork: DiscordArtworkConfig::default(),
        }
    }
}

#[derive(Deserialize)]
struct SettingsDeser {
    #[serde(default)]
    audio_cache: bool,
    #[serde(default = "default_true")]
    art_hq: bool,
    #[serde(default)]
    art_palette: bool,
    #[serde(default)]
    provider: Provider,
    #[serde(default)]
    discord_rpc: Option<bool>,
    #[serde(default)]
    discord: DiscordConfigDeser,
}

impl<'de> Deserialize<'de> for Settings {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = SettingsDeser::deserialize(deserializer)?;
        // Canonical setting is `discord.enabled`; fall back to legacy `discord_rpc` only when omitted.
        let enabled = raw.discord.enabled.or(raw.discord_rpc).unwrap_or(false);
        Ok(Self {
            audio_cache: raw.audio_cache,
            art_hq: raw.art_hq,
            art_palette: raw.art_palette,
            provider: raw.provider,
            discord: DiscordConfig {
                enabled,
                artwork_enabled: raw.discord.artwork_enabled,
                artwork: raw.discord.artwork,
            },
        })
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            audio_cache: false,
            art_hq: true,
            art_palette: false,
            provider: Provider::YtMusic,
            discord: DiscordConfig::default(),
        }
    }
}

impl Settings {
    pub fn load() -> Self {
        let defaults = Self::default();

        let Ok(content) = fs::read_to_string(config_path()) else {
            return defaults;
        };

        // 1. Try parsing as TOML with Serde (includes legacy `discord_rpc` migration)
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
                "discord_rpc" => {
                    settings.discord.enabled = value.trim() == "true";
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

pub fn discord_artwork_cache_dir() -> PathBuf {
    cache_base_dir().join("discord").join("artwork")
}

pub fn cache_bytes() -> u64 {
    dir_size(&artwork_cache_dir())
        + dir_size(&audio_cache_dir())
        + dir_size(&discord_artwork_cache_dir())
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
    let _ = clean_stale_part_files(&discord_artwork_cache_dir());
    let _ = clean_stale_part_files(&discord_artwork_cache_dir().join("images"));
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

    let discord_art_dir = discord_artwork_cache_dir();
    if discord_art_dir.exists()
        && let Err(e) = fs::remove_dir_all(&discord_art_dir)
    {
        err = Some(e);
    }
    // Clean up empty parent discord/ cache directory if present
    let _ = fs::remove_dir(cache_base_dir().join("discord"));

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
            discord: DiscordConfig {
                enabled: true,
                artwork_enabled: true,
                artwork: DiscordArtworkConfig::default(),
            },
        };

        let toml_str = toml::to_string_pretty(&settings).expect("serialize toml");
        assert!(toml_str.contains("audio_cache = true"));
        assert!(toml_str.contains("art_hq = false"));
        assert!(toml_str.contains("art_palette = true"));
        assert!(toml_str.contains("provider = \"spotify\""));
        assert!(!toml_str.contains("discord_rpc"));
        assert!(toml_str.contains("[discord]"));
        assert!(toml_str.contains("enabled = true"));
        assert!(toml_str.contains("[discord.artwork]"));

        let loaded: Settings = toml::from_str(&toml_str).expect("deserialize toml");
        assert_eq!(loaded, settings);
    }

    #[test]
    fn test_legacy_discord_rpc_toml_migrates_to_discord_enabled() {
        let legacy_toml = "provider = \"ytmusic\"\ndiscord_rpc = true\n";
        let loaded: Settings = toml::from_str(legacy_toml).expect("migrate legacy discord_rpc");
        assert!(loaded.discord.enabled);

        // Explicit [discord].enabled takes precedence over a stale top-level discord_rpc flag
        let explicit_disabled = "discord_rpc = true\n\n[discord]\nenabled = false\n";
        let loaded_disabled: Settings =
            toml::from_str(explicit_disabled).expect("deserialize explicit override");
        assert!(!loaded_disabled.discord.enabled);
    }

    #[test]
    fn test_phase7_discord_artwork_toml_spec() {
        let toml_spec = r#"
[discord]
enabled = true
artwork_enabled = true

[discord.artwork]
cache_max_size_mb = 250
max_download_size_mb = 8
image_size = 1024

primary_host = "catbox"
fallback_host = "litterbox"
litterbox_expiry = "72h"
"#;
        let loaded: Settings = toml::from_str(toml_spec).expect("deserialize phase 7 toml");
        assert!(loaded.discord.enabled);
        assert!(loaded.discord.artwork_enabled);
        assert_eq!(loaded.discord.artwork.cache_max_size_mb, 250);
        assert_eq!(loaded.discord.artwork.max_download_size_mb, 8);
        assert_eq!(loaded.discord.artwork.image_size, 1024);
        assert_eq!(loaded.discord.artwork.primary_host, "catbox");
        assert_eq!(loaded.discord.artwork.fallback_host, "litterbox");
        assert_eq!(
            loaded.discord.artwork.litterbox_expiry,
            LitterboxExpiry::SeventyTwoHours
        );

        assert_eq!(loaded.discord.artwork.cache_max_bytes(), 250 * 1024 * 1024);
        assert_eq!(loaded.discord.artwork.max_download_bytes(), 8 * 1024 * 1024);
        assert_eq!(
            loaded.discord.artwork.litterbox_duration_seconds(),
            72 * 3600
        );
        assert_eq!(loaded.discord.artwork.litterbox_time_str(), "72h");

        let defaults = DiscordConfig::default();
        assert!(!defaults.enabled);
        assert!(defaults.artwork_enabled);
        assert_eq!(defaults.artwork.primary_host, "imgdb");
        assert_eq!(defaults.artwork.fallback_host, "catbox");
        assert_eq!(
            defaults.artwork.litterbox_expiry,
            LitterboxExpiry::SeventyTwoHours
        );
    }

    #[test]
    fn test_discord_artwork_expiry_variations() {
        let mut art_cfg = DiscordArtworkConfig {
            litterbox_expiry: LitterboxExpiry::OneHour,
            ..Default::default()
        };
        assert_eq!(art_cfg.litterbox_duration_seconds(), 3600);
        assert_eq!(art_cfg.litterbox_time_str(), "1h");

        art_cfg.litterbox_expiry = LitterboxExpiry::TwelveHours;
        assert_eq!(art_cfg.litterbox_duration_seconds(), 12 * 3600);
        assert_eq!(art_cfg.litterbox_time_str(), "12h");

        art_cfg.litterbox_expiry = LitterboxExpiry::TwentyFourHours;
        assert_eq!(art_cfg.litterbox_duration_seconds(), 24 * 3600);
        assert_eq!(art_cfg.litterbox_time_str(), "24h");

        art_cfg.litterbox_expiry = LitterboxExpiry::SeventyTwoHours;
        assert_eq!(art_cfg.litterbox_duration_seconds(), 72 * 3600);
        assert_eq!(art_cfg.litterbox_time_str(), "72h");

        // Unsupported retention durations must fail validation at deserialization time
        let unsupported = "[discord.artwork]\nlitterbox_expiry = \"48h\"\n";
        assert!(toml::from_str::<Settings>(unsupported).is_err());
    }

    #[test]
    fn test_settings_missing_discord_rpc_defaults_to_false() {
        let existing_toml =
            "audio_cache = true\nart_hq = true\nart_palette = false\nprovider = \"ytmusic\"\n";
        let loaded: Settings = toml::from_str(existing_toml).expect("deserialize existing toml");
        assert!(
            !loaded.discord.enabled,
            "Existing config without discord setting must default to false"
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
                    "discord_rpc" => settings.discord.enabled = value.trim() == "true",
                    _ => {}
                }
            }
        }

        assert!(settings.audio_cache);
        assert!(!settings.art_hq);
        assert!(settings.art_palette);
        assert_eq!(settings.provider, Provider::Spotify);
        assert!(settings.discord.enabled);
    }

    #[test]
    fn test_clear_caches_result() {
        let _guard = crate::artwork::ARTWORK_TEST_LOCK.lock().unwrap();
        let result = clear_caches();
        assert!(result.is_ok(), "clear_caches must return Ok");
    }

    #[test]
    fn test_clear_caches_includes_discord_artwork_and_updates_cache_bytes() {
        let _guard = crate::artwork::ARTWORK_TEST_LOCK.lock().unwrap();
        let discord_dir = discord_artwork_cache_dir();
        let test_img_dir = discord_dir.join("images");
        fs::create_dir_all(&test_img_dir).expect("create discord cache dir");
        fs::write(discord_dir.join("index.json"), b"{\"entries\":{}}").expect("write index");
        fs::write(test_img_dir.join("test.jpg"), b"image-payload-bytes").expect("write img");

        // cache_bytes must count Discord artwork directory
        assert!(cache_bytes() >= b"image-payload-bytes".len() as u64);

        // clear_caches must successfully clear Discord artwork cache root
        let result = clear_caches();
        assert!(result.is_ok(), "clear_caches must return Ok");
        assert!(
            !discord_dir.exists(),
            "discord artwork cache must be removed by clear_caches"
        );
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
