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
        unsafe {
            std::env::remove_var("ANSIMUZ_DATA_DIR");
        }
    }
}
