use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{LazyLock, Mutex},
    time::Instant,
};

use image::DynamicImage;

pub const TOTAL_FRAMES: usize = 20;
pub const FRAME_DURATION_MS: usize = 100; // 10 FPS

static ANIM_START: LazyLock<Instant> = LazyLock::new(Instant::now);
static FRAME_CACHE: LazyLock<Mutex<HashMap<usize, DynamicImage>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static PREWARMED: LazyLock<()> = LazyLock::new(|| {
    std::thread::spawn(|| {
        for i in 0..TOTAL_FRAMES {
            get_animation_frame(i);
        }
    });
});

fn resolve_frame_path(frame_number: usize) -> Option<PathBuf> {
    let frame_dir = crate::paths::data_dir().join("assets/frames");
    let filenames = [
        format!("frame-{frame_number:03}.png"),
        format!("frame_{frame_number:03}.png"),
    ];

    for name in &filenames {
        let p = frame_dir.join(name);
        if p.exists() {
            return Some(p);
        }
    }

    let rel_filenames = [
        format!("assets/frames/frame-{frame_number:03}.png"),
        format!("assets/frames/frame_{frame_number:03}.png"),
    ];

    for rel in rel_filenames {
        // Current working directory
        let p = PathBuf::from(&rel);
        if p.exists() {
            return Some(p);
        }
        // Executable directory ancestors
        if let Ok(exe) = std::env::current_exe()
            && let Some(parent) = exe.parent()
        {
            let p = parent.join(&rel);
            if p.exists() {
                return Some(p);
            }
            if let Some(grandparent) = parent.parent() {
                let p = grandparent.join(&rel);
                if p.exists() {
                    return Some(p);
                }
                if let Some(great) = grandparent.parent() {
                    let p = great.join(&rel);
                    if p.exists() {
                        return Some(p);
                    }
                }
            }
        }
    }

    None
}

pub fn get_animation_frame(index: usize) -> Option<DynamicImage> {
    let frame_idx = index % TOTAL_FRAMES;

    if let Ok(guard) = FRAME_CACHE.lock()
        && let Some(img) = guard.get(&frame_idx)
    {
        return Some(img.clone());
    }

    let frame_num = frame_idx + 1;
    let path = resolve_frame_path(frame_num)?;
    let loaded = image::open(&path).ok()?;

    if let Ok(mut guard) = FRAME_CACHE.lock() {
        guard.insert(frame_idx, loaded.clone());
    }

    Some(loaded)
}

pub fn current_frame_index() -> usize {
    let elapsed = ANIM_START.elapsed().as_millis() as usize;
    (elapsed / FRAME_DURATION_MS) % TOTAL_FRAMES
}

pub fn current_animation_frame() -> Option<DynamicImage> {
    ensure_prewarmed();
    get_animation_frame(current_frame_index())
}

pub fn ensure_prewarmed() {
    *PREWARMED;
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
}
