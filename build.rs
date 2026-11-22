use std::env;
use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(
        env::var("CARGO_MANIFEST_DIR").unwrap()
    );

    let mpv_dir = manifest_dir.join("native").join("mpv");
    let include_dir = mpv_dir.join("include");
    let source_file = mpv_dir.join("src").join("mpv_bridge.c");
    let lib_dir = mpv_dir.join("lib");

    println!("cargo:rerun-if-changed={}", source_file.display());
    println!(
        "cargo:rerun-if-changed={}",
        include_dir.join("ansimuz_mpv.h").display()
    );

    // Compile the ANSImuz C bridge.
    cc::Build::new()
        .file(&source_file)
        .include(&include_dir)
        .warnings(true)
        .compile("ansimuz_mpv_bridge");

    // Link against the libmpv import library.
    if cfg!(target_env = "msvc") {
        println!(
            "cargo:rustc-link-search=native={}",
            lib_dir.display()
        );
        println!("cargo:rustc-link-lib=dylib=mpv");
    }
}