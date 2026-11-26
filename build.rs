use std::{env, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=MPV_LIB_DIR");
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_LEGACY_YTDL");

    if env::var_os("CARGO_FEATURE_NATIVE_MPV").is_none() {
        return;
    }

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap();

    let mpv_dir = manifest_dir.join("native").join("mpv");
    let include_dir = mpv_dir.join("include");
    let source_file = mpv_dir.join("src").join("mpv_bridge.c");
    let lib_dir = mpv_dir.join("lib");

    println!("cargo:rerun-if-changed={}", source_file.display());
    println!(
        "cargo:rerun-if-changed={}",
        include_dir.join("ansimuz_mpv.h").display()
    );

    // Compile the ANSImuz C bridge
    let mut build = cc::Build::new();
    build
        .file(&source_file)
        .include(&include_dir)
        .warnings(true);
    if env::var_os("CARGO_FEATURE_LEGACY_YTDL").is_some() {
        build.define("ANSIMUZ_LEGACY_YTDL", "1");
    }
    build.compile("ansimuz_mpv_bridge");

    // Link against libmpv
    if let Ok(dir) = env::var("MPV_LIB_DIR") {
        // Manual override for custom installation paths
        println!("cargo:rustc-link-search=native={dir}");
        println!("cargo:rustc-link-lib=dylib=mpv");
    } else if target_os == "windows" {
        println!("cargo:rustc-link-search=native={}", lib_dir.display());
        println!("cargo:rustc-link-lib=dylib=mpv");

        // Copy libmpv-2.dll next to compiled binaries for direct execution
        let dll_src = lib_dir.join("libmpv-2.dll");
        if dll_src.exists()
            && let Ok(out_dir) = env::var("OUT_DIR")
        {
            let out_path = PathBuf::from(out_dir);
            if let Some(profile_dir) = out_path.ancestors().nth(3) {
                let dll_dest = profile_dir.join("libmpv-2.dll");
                let _ = std::fs::copy(&dll_src, &dll_dest);
            }
        }
    } else if pkg_config::Config::new().probe("mpv").is_err() {
        println!("cargo:warning=pkg-config could not find mpv, falling back to -lmpv");
        println!("cargo:rustc-link-lib=dylib=mpv");
    }
}
