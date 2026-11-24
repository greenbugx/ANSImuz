use std::{env, path::PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap();

    let mpv_dir = manifest_dir.join("native").join("mpv");
    let include_dir = mpv_dir.join("include");
    let source_file = mpv_dir.join("src").join("mpv_bridge.c");
    let lib_dir = mpv_dir.join("lib");

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=MPV_LIB_DIR");
    println!("cargo:rerun-if-changed={}", source_file.display());
    println!(
        "cargo:rerun-if-changed={}",
        include_dir.join("ansimuz_mpv.h").display()
    );

    // Compile the ANSImuz C bridge
    cc::Build::new()
        .file(&source_file)
        .include(&include_dir)
        .warnings(true)
        .compile("ansimuz_mpv_bridge");

    // Link against libmpv
    if let Ok(dir) = env::var("MPV_LIB_DIR") {
        // Manual override for custom installation paths
        println!("cargo:rustc-link-search=native={dir}");
        println!("cargo:rustc-link-lib=dylib=mpv");
    } else if target_os == "windows" {
        println!("cargo:rustc-link-search=native={}", lib_dir.display());
        println!("cargo:rustc-link-lib=dylib=mpv");
    } else if pkg_config::Config::new().probe("mpv").is_err() {
        println!("cargo:warning=pkg-config could not find mpv, falling back to -lmpv");
        println!("cargo:rustc-link-lib=dylib=mpv");
    }
}
