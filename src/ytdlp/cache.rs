use std::{
    collections::HashMap,
    sync::{LazyLock, Mutex},
    time::{Duration, SystemTime},
};

use super::types::ResolvedStream;

pub const MAX_STREAM_CACHE_ENTRIES: usize = 200;
const EXPIRY_SAFETY_MARGIN: Duration = Duration::from_secs(5 * 60);
const MAX_STREAM_TTL: Duration = Duration::from_secs(30 * 60);

struct CacheEntry {
    stream: ResolvedStream,
    inserted_at: SystemTime,
    ttl: Duration,
}

impl CacheEntry {
    fn is_fresh(&self, now: SystemTime) -> bool {
        if self.ttl.is_zero() {
            return false;
        }
        now.duration_since(self.inserted_at)
            .map(|elapsed| elapsed < self.ttl)
            .unwrap_or(true)
    }
}

/// Bounded in-memory LRU cache of resolved direct stream URLs keyed by video ID
pub struct ResolvedStreamCache {
    entries: HashMap<String, CacheEntry>,
    order: Vec<String>,
    capacity: usize,
}

impl ResolvedStreamCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: HashMap::new(),
            order: Vec::new(),
            capacity: capacity.max(1),
        }
    }

    pub fn ttl_for(stream: &ResolvedStream, now: SystemTime) -> Duration {
        match stream.expires_at {
            Some(exp) => exp
                .duration_since(now)
                .unwrap_or(Duration::ZERO)
                .saturating_sub(EXPIRY_SAFETY_MARGIN)
                .min(MAX_STREAM_TTL),
            None => MAX_STREAM_TTL,
        }
    }

    pub fn get(&mut self, video_id: &str, now: SystemTime) -> Option<ResolvedStream> {
        let fresh = self
            .entries
            .get(video_id)
            .map(|entry| entry.is_fresh(now))?;
        if !fresh {
            self.remove_entry(video_id);
            return None;
        }

        if let Some(pos) = self.order.iter().position(|k| k == video_id) {
            let k = self.order.remove(pos);
            self.order.push(k);
        }

        self.entries.get(video_id).map(|e| e.stream.clone())
    }

    pub fn insert(&mut self, video_id: String, stream: ResolvedStream, now: SystemTime) {
        let ttl = Self::ttl_for(&stream, now);
        if ttl.is_zero() {
            self.remove_entry(&video_id);
            return;
        }

        let entry = CacheEntry {
            stream,
            inserted_at: now,
            ttl,
        };

        if self.entries.contains_key(&video_id) {
            self.entries.insert(video_id.clone(), entry);
            if let Some(pos) = self.order.iter().position(|k| k == &video_id) {
                let k = self.order.remove(pos);
                self.order.push(k);
            }
            return;
        }

        while self.entries.len() >= self.capacity {
            if let Some(oldest) = self.order.first().cloned() {
                self.remove_entry(&oldest);
            } else {
                break;
            }
        }

        self.order.push(video_id.clone());
        self.entries.insert(video_id, entry);
    }

    pub fn evict(&mut self, video_id: &str) {
        self.remove_entry(video_id);
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.order.clear();
    }

    fn remove_entry(&mut self, video_id: &str) {
        self.entries.remove(video_id);
        if let Some(pos) = self.order.iter().position(|k| k == video_id) {
            self.order.remove(pos);
        }
    }
}

static STREAM_CACHE: LazyLock<Mutex<ResolvedStreamCache>> =
    LazyLock::new(|| Mutex::new(ResolvedStreamCache::new(MAX_STREAM_CACHE_ENTRIES)));

pub fn extract_video_id(url: &str) -> Option<String> {
    let parsed = reqwest::Url::parse(url.trim()).ok()?;
    if parsed.scheme() != "https" || !parsed.username().is_empty() || parsed.password().is_some() {
        return None;
    }

    let host = parsed
        .host_str()?
        .trim_end_matches('.')
        .to_ascii_lowercase();
    match host.as_str() {
        "youtu.be" => parsed
            .path_segments()?
            .next()
            .filter(|id| !id.is_empty())
            .map(|id| id.to_string()),
        "youtube.com" | "www.youtube.com" | "music.youtube.com" => {
            for (k, v) in parsed.query_pairs() {
                if k == "v" && !v.is_empty() {
                    return Some(v.into_owned());
                }
            }
            let mut segments = parsed.path_segments()?;
            match segments.next() {
                Some("shorts" | "embed" | "live") => segments
                    .next()
                    .filter(|id| !id.is_empty())
                    .map(|id| id.to_string()),
                _ => None,
            }
        }
        _ => None,
    }
}

pub fn is_youtube_watch_url(url: &str) -> bool {
    extract_video_id(url).is_some()
}

pub fn get_cached_stream(watch_url: &str) -> Option<ResolvedStream> {
    let key = extract_video_id(watch_url)?;
    let mut guard = match STREAM_CACHE.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    guard.get(&key, SystemTime::now())
}

pub fn cache_stream(watch_url: &str, stream: &ResolvedStream) {
    let Some(key) = extract_video_id(watch_url).or_else(|| stream.video_id.clone()) else {
        return;
    };
    let mut guard = match STREAM_CACHE.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    guard.insert(key, stream.clone(), SystemTime::now());
}

pub fn evict_cached_stream(watch_url: &str) {
    let Some(key) = extract_video_id(watch_url) else {
        return;
    };
    let mut guard = match STREAM_CACHE.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    guard.evict(&key);
}

pub fn clear_stream_cache() {
    let mut guard = match STREAM_CACHE.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    guard.clear();
}

pub fn stream_cache_len() -> usize {
    let guard = match STREAM_CACHE.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    guard.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_stream(video_id: &str, expires_at: Option<SystemTime>) -> ResolvedStream {
        ResolvedStream {
            url: format!("https://rr1.googlevideo.com/videoplayback?id={video_id}"),
            headers: vec![("User-Agent".into(), "test".into())],
            expires_at,
            ext: Some("webm".into()),
            acodec: Some("opus".into()),
            abr: Some(128.0),
            duration_s: Some(210.0),
            video_id: Some(video_id.into()),
            title: Some("Sample".into()),
            ytdlp_version: "2026.11.23".into(),
            js_runtime: "system-node".into(),
        }
    }

    #[test]
    fn test_extract_video_id_supports_youtube_variants_and_rejects_others() {
        assert_eq!(
            extract_video_id("https://www.youtube.com/watch?v=Go2MaihKX9o"),
            Some("Go2MaihKX9o".into())
        );
        assert_eq!(
            extract_video_id("https://music.youtube.com/watch?v=dQw4w9WgXcQ&si=abc"),
            Some("dQw4w9WgXcQ".into())
        );
        assert_eq!(
            extract_video_id("https://youtu.be/kJQP7kiw5Fk?t=12"),
            Some("kJQP7kiw5Fk".into())
        );
        assert_eq!(
            extract_video_id("https://youtube.com/shorts/abc123"),
            Some("abc123".into())
        );
        assert_eq!(extract_video_id("http://youtube.com/watch?v=123"), None);
        assert_eq!(extract_video_id("https://example.com/audio.mp3"), None);
    }

    #[test]
    fn test_cache_ttl_clamps_to_expiry_margin_and_evicts_expired() {
        let base = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        let mut cache = ResolvedStreamCache::new(4);

        // Expires in 20 minutes -> effective TTL is 15 minutes after 5-minute safety margin
        let expires_20m = base + Duration::from_secs(20 * 60);
        cache.insert(
            "vid1".into(),
            sample_stream("vid1", Some(expires_20m)),
            base,
        );
        assert!(
            cache
                .get("vid1", base + Duration::from_secs(14 * 60))
                .is_some()
        );
        assert!(
            cache
                .get("vid1", base + Duration::from_secs(15 * 60))
                .is_none()
        );

        // Expires within 5 minutes -> not cached
        cache.insert(
            "vid2".into(),
            sample_stream("vid2", Some(base + Duration::from_secs(4 * 60))),
            base,
        );
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn test_cache_caps_ttl_at_30_minutes_and_evicts_lru() {
        let base = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        let mut cache = ResolvedStreamCache::new(2);

        // Expires in 6 hours -> capped at 30 minutes
        let s1 = sample_stream("v1", Some(base + Duration::from_secs(6 * 3600)));
        let s2 = sample_stream("v2", None);
        let s3 = sample_stream("v3", None);

        cache.insert("v1".into(), s1, base);
        assert!(
            cache
                .get("v1", base + Duration::from_secs(29 * 60))
                .is_some()
        );
        assert!(
            cache
                .get("v1", base + Duration::from_secs(31 * 60))
                .is_none()
        );

        cache.insert("v1".into(), sample_stream("v1", None), base);
        cache.insert("v2".into(), s2, base);
        // Touch v1 so v2 becomes oldest
        assert!(cache.get("v1", base).is_some());
        cache.insert("v3".into(), s3, base);
        assert!(cache.get("v2", base).is_none());
        assert!(cache.get("v1", base).is_some());
        assert!(cache.get("v3", base).is_some());
    }
}
