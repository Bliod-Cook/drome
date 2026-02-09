use std::{env, path::PathBuf};

fn main() {
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if target_os != "windows" || target_env != "msvc" {
        return;
    }

    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"))
        .join("windows")
        .join("cherry-app.manifest");
    let manifest_display = manifest.to_string_lossy();
    let manifest_arg = if manifest_display.contains(' ') {
        format!("/MANIFESTINPUT:\"{}\"", manifest_display)
    } else {
        format!("/MANIFESTINPUT:{}", manifest_display)
    };

    println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
    println!("cargo:rustc-link-arg-bins={manifest_arg}");
    println!("cargo:rerun-if-changed={}", manifest.display());
    println!("cargo:rerun-if-changed=build.rs");
}
