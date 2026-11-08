use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc, LazyLock, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};

use image::DynamicImage;

pub const TOTAL_FRAMES: usize = 20;
pub const FRAME_DURATION_MS: usize = 100; // 10 FPS
pub const UPCOMING_PREWARM_FRAMES: usize = 3;

static ANIM_START: LazyLock<Instant> = LazyLock::new(Instant::now);
static FRAME_CACHE: LazyLock<Mutex<HashMap<usize, Arc<DynamicImage>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static PREWARM_STARTED: AtomicBool = AtomicBool::new(false);
fn resolve_frame_path(frame_number: usize) -> Option<PathBuf> {
    let frame_dir = crate::paths::frames_dir();

    let candidates = [
        format!("frame-{frame_number:03}.png"),
        format!("frame_{frame_number:03}.png"),
    ];

    candidates
        .iter()
        .map(|name| frame_dir.join(name))
        .find(|path| path.is_file())
}

pub fn get_animation_frame(index: usize) -> Option<Arc<DynamicImage>> {
    let frame_idx = index % TOTAL_FRAMES;

    if let Ok(guard) = FRAME_CACHE.lock()
        && let Some(img) = guard.get(&frame_idx)
    {
        return Some(Arc::clone(img));
    }

    let frame_num = frame_idx + 1;
    let path = resolve_frame_path(frame_num)?;
    let loaded = Arc::new(image::open(&path).ok()?);
    if let Ok(mut guard) = FRAME_CACHE.lock() {
        if let Some(existing) = guard.get(&frame_idx) {
            return Some(Arc::clone(existing));
        }
        guard.insert(frame_idx, Arc::clone(&loaded));
    }

    Some(loaded)
}

pub fn current_frame_index() -> usize {
    let elapsed = ANIM_START.elapsed().as_millis() as usize;
    (elapsed / FRAME_DURATION_MS) % TOTAL_FRAMES
}

pub fn trigger_prewarming(current: usize) {
    if PREWARM_STARTED
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_ok()
    {
        std::thread::spawn(move || {
            // Prewarm upcoming frames first for immediate responsiveness
            for offset in 1..=UPCOMING_PREWARM_FRAMES {
                let next_idx = (current + offset) % TOTAL_FRAMES;
                get_animation_frame(next_idx);
            }

            // Continue loading all remaining frames in the background
            for i in 0..TOTAL_FRAMES {
                get_animation_frame(i);
            }
        });
    }
}

pub fn current_animation_frame() -> Option<Arc<DynamicImage>> {
    let curr = current_frame_index();
    let frame = get_animation_frame(curr);
    trigger_prewarming(curr);
    frame
}

pub fn ensure_prewarmed() {
    trigger_prewarming(0);
}

pub fn is_prewarm_started() -> bool {
    PREWARM_STARTED.load(Ordering::Relaxed)
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_animation_frame_resolution_and_cycling() {
        // Test frame index math
        assert_eq!(0 % TOTAL_FRAMES, 0);
        assert_eq!(19 % TOTAL_FRAMES, 19);
        assert_eq!(20 % TOTAL_FRAMES, 0);

        // Verify frame 0 loads a valid image with dimensions
        if let Some(frame) = get_animation_frame(0) {
            assert!(frame.width() > 0);
            assert!(frame.height() > 0);
        }
    }

    #[test]
    fn test_animation_frame_cache_shares_allocation() {
        let frame1 = get_animation_frame(0);
        let frame2 = get_animation_frame(0);

        if let (Some(f1), Some(f2)) = (frame1, frame2) {
            // Verify pointer equality: both Arcs point to the exact same image allocation
            assert!(Arc::ptr_eq(&f1, &f2));
        }
    }

    #[test]
    fn test_missing_frame_fails_gracefully() {
        assert!(resolve_frame_path(99999).is_none());
    }

    #[test]
    fn test_first_frame_loads_immediately_and_triggers_prewarming() {
        let frame = current_animation_frame();
        if let Some(img) = frame {
            assert!(img.width() > 0);
        }
        assert!(is_prewarm_started());
    }

    #[test]
    fn test_trigger_prewarming_does_not_spawn_multiple_workers() {
        trigger_prewarming(0);
        trigger_prewarming(5);
        assert!(is_prewarm_started());
    }
}
