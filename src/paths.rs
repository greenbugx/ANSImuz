use std::path::PathBuf;

pub fn data_dir() -> PathBuf {
    if let Ok(path) = std::env::var("ANSIMUZ_DATA_DIR") {
        return PathBuf::from(path);
    }

    let installed = PathBuf::from("/usr/share/ansimuz");

    if installed.is_dir() {
        return installed;
    }

    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_data_dir_resolution() {
        let dir = data_dir();
        assert!(dir.exists() || dir == std::path::Path::new("/usr/share/ansimuz"));
    }

    #[test]
    fn test_data_dir_override() {
        unsafe {
            std::env::set_var("ANSIMUZ_DATA_DIR", "/tmp/ansimuz-custom-data");
        }
        let dir = data_dir();
        assert_eq!(dir, PathBuf::from("/tmp/ansimuz-custom-data"));
        unsafe {
            std::env::remove_var("ANSIMUZ_DATA_DIR");
        }
    }
}
