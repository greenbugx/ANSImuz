use std::{
    env,
    path::{Path, PathBuf},
};

fn has_data(path: &Path) -> bool {
    path.join("backend").is_dir() && path.join("assets").is_dir()
}

pub fn data_dir() -> PathBuf {
    // allow explicit override
    if let Ok(path) = env::var("ANSIMUZ_DATA_DIR") {
        return PathBuf::from(path);
    }
    // Development: running from the project directory.
    if let Ok(cwd) = env::current_dir()
        && has_data(&cwd)
    {
        return cwd;
    }

    // Development: running the compiled binary from target/debug
    // or target/release, even when launched outside the project.
    if let Ok(exe) = env::current_exe() {
        for ancestor in exe.ancestors().skip(1) {
            if has_data(ancestor) {
                return ancestor.to_path_buf();
            }
        }
    }

    // compile-time configured default
    if let Some(path) = option_env!("ANSIMUZ_DATA_DIR") {
        let p = PathBuf::from(path);
        if has_data(&p) {
            return p;
        }
    }

    // installed application
    let installed = PathBuf::from("/usr/share/ansimuz");
    if has_data(&installed) {
        return installed;
    }
    env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

pub fn backend_dir() -> PathBuf {
    data_dir().join("backend")
}

pub fn assets_dir() -> PathBuf {
    data_dir().join("assets")
}

pub fn frames_dir() -> PathBuf {
    data_dir().join("assets/frames")
}

pub fn runtime_dir() -> PathBuf {
    data_dir().join("runtime")
}

pub fn vendor_dir() -> PathBuf {
    data_dir().join("vendor")
}

pub fn site_packages_dir() -> PathBuf {
    data_dir().join("Lib").join("site-packages")
}

pub fn user_data_dir() -> PathBuf {
    if let Ok(path) = env::var("ANSIMUZ_USER_DATA_DIR") {
        return PathBuf::from(path);
    }

    #[cfg(windows)]
    {
        if let Ok(local_app_data) = env::var("LOCALAPPDATA") {
            return PathBuf::from(local_app_data).join("ansimuz");
        }
        if let Ok(app_data) = env::var("APPDATA") {
            return PathBuf::from(app_data).join("ansimuz");
        }
    }

    if let Ok(xdg_data) = env::var("XDG_DATA_HOME") {
        return PathBuf::from(xdg_data).join("ansimuz");
    }

    env::var("HOME")
        .map(|h| {
            PathBuf::from(h)
                .join(".local")
                .join("share")
                .join("ansimuz")
        })
        .unwrap_or_else(|_| env::temp_dir().join("ansimuz"))
}

pub fn overlay_base_dir() -> PathBuf {
    user_data_dir().join("ytdlp-overlay")
}

pub fn active_overlay_dir() -> Option<PathBuf> {
    let pointer = overlay_base_dir().join("current");
    let raw = std::fs::read_to_string(pointer).ok()?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let candidate = PathBuf::from(trimmed);
    let resolved = if candidate.is_absolute() {
        candidate
    } else {
        overlay_base_dir().join(candidate)
    };
    resolved.is_dir().then_some(resolved)
}

pub fn bundled_python_home_for(dir: &Path) -> Option<PathBuf> {
    if !dir.join("Lib").join("site-packages").is_dir() {
        return None;
    }
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let is_stdlib_zip = name.starts_with("python") && name.ends_with(".zip");
        let is_pth = name.starts_with("python") && name.ends_with("._pth");
        if (is_stdlib_zip || is_pth) && entry.path().is_file() {
            return Some(dir.to_path_buf());
        }
    }
    None
}

// Must run before any PyO3 Python::attach call so auto-initialize picks up the bundled interpreter
pub fn configure_python_home() {
    if env::var_os("PYTHONHOME").is_some() {
        return;
    }
    if let Some(home) = bundled_python_home_for(&data_dir()) {
        unsafe {
            env::set_var("PYTHONHOME", home);
        }
    }
}
#[cfg(test)]
pub static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_data_dir_resolution() {
        let _guard = ENV_LOCK.lock().unwrap();
        let dir = data_dir();
        assert!(dir.exists() || dir == std::path::Path::new("/usr/share/ansimuz"));
    }

    #[test]
    fn test_data_dir_override() {
        let _guard = ENV_LOCK.lock().unwrap();
        unsafe {
            std::env::set_var("ANSIMUZ_DATA_DIR", "/tmp/ansimuz-custom-data");
        }
        let dir = data_dir();
        assert_eq!(dir, PathBuf::from("/tmp/ansimuz-custom-data"));
        assert_eq!(
            backend_dir(),
            PathBuf::from("/tmp/ansimuz-custom-data/backend")
        );
        assert_eq!(
            assets_dir(),
            PathBuf::from("/tmp/ansimuz-custom-data/assets")
        );
        assert_eq!(
            frames_dir(),
            PathBuf::from("/tmp/ansimuz-custom-data/assets/frames")
        );
        assert_eq!(
            runtime_dir(),
            PathBuf::from("/tmp/ansimuz-custom-data/runtime")
        );
        assert_eq!(
            vendor_dir(),
            PathBuf::from("/tmp/ansimuz-custom-data/vendor")
        );
        assert_eq!(
            site_packages_dir(),
            PathBuf::from("/tmp/ansimuz-custom-data/Lib/site-packages")
        );
        unsafe {
            std::env::remove_var("ANSIMUZ_DATA_DIR");
        }
    }

    #[test]
    fn test_bundled_python_home_detection() {
        let dir = std::env::temp_dir().join("ansimuz-test-pyhome");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(bundled_python_home_for(&dir), None);

        std::fs::create_dir_all(dir.join("Lib").join("site-packages")).unwrap();
        assert_eq!(bundled_python_home_for(&dir), None);

        std::fs::write(dir.join("python314.zip"), b"zip").unwrap();
        assert_eq!(bundled_python_home_for(&dir), Some(dir.clone()));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
