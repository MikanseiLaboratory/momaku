use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    // Run before `tauri_build` so `bundle.resources` paths exist when Tauri validates the merged config.
    copy_windows_runtime_dlls_next_to_exe();
    tauri_build::build();
}

/// surfman loads `libEGL.dll` from the DLL search path (typically the directory of `momaku.exe`).
/// mozangle builds those into its crate `OUT_DIR`; copy them next to the final `momaku.exe` output
/// (`target/<profile>/` for native builds, or `target/<triple>/<profile>/` when cross-compiling).
///
/// Also copy the vendored NDI runtime DLL next to the exe so `cargo run --release` and unbundled
/// `momaku.exe` resolve `Processing.NDI.Lib.x64.dll` without relying on the installer alone.
fn copy_windows_runtime_dlls_next_to_exe() {
    if !cfg!(target_os = "windows") {
        return;
    }
    let Ok(target) = env::var("TARGET") else {
        return;
    };
    if target != "x86_64-pc-windows-msvc" {
        println!("cargo:warning=momaku build.rs: skipping mozangle DLL copy for target {target}");
        return;
    }

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let profile = env::var("PROFILE").unwrap_or_else(|_| "debug".into());
    let host = env::var("HOST").unwrap_or_default();

    let mut target_roots: BTreeSet<PathBuf> = BTreeSet::new();
    if let Ok(td) = env::var("CARGO_TARGET_DIR") {
        target_roots.insert(PathBuf::from(td));
    }
    if let Ok(ws) = env::var("CARGO_WORKSPACE_DIR") {
        target_roots.insert(PathBuf::from(ws).join("target"));
    }
    if let Some(parent) = manifest_dir.parent() {
        target_roots.insert(parent.join("target"));
    }
    target_roots.insert(manifest_dir.join("target"));

    let mut moz_out: Option<PathBuf> = None;
    let mut dest_dir: Option<PathBuf> = None;

    for root in &target_roots {
        let dest = if host == target {
            root.join(&profile)
        } else {
            root.join(&target).join(&profile)
        };
        let build_root = dest.join("build");
        if let Some(out) = find_mozangle_out_dir(&build_root) {
            moz_out = Some(out);
            dest_dir = Some(dest);
            break;
        }
    }

    let Some(mozangle_out_dir) = moz_out else {
        let listed = target_roots
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        panic!(
            "momaku Windows build: mozangle `out` with libEGL.dll not found under any of: [{}]. \
             Build the `servo` dependency first (Windows + `no-wgl` / mozangle `build_dlls`).",
            listed
        );
    };
    let dest_dir = dest_dir.expect("dest_dir set with moz_out");

    for name in ["libEGL.dll", "libGLESv2.dll"] {
        let src = mozangle_out_dir.join(name);
        if !src.is_file() {
            panic!(
                "momaku Windows build: missing mozangle artifact {}",
                src.display()
            );
        }
        let dst = dest_dir.join(name);
        fs::copy(&src, &dst).unwrap_or_else(|e| {
            panic!(
                "momaku Windows build: failed to copy {} -> {}: {e}",
                src.display(),
                dst.display()
            )
        });
        println!("cargo:rerun-if-changed={}", src.display());
    }

    let ndi_src = manifest_dir
        .join("../third_party/ndi-sdk-6/lib/x64/Processing.NDI.Lib.x64.dll")
        .canonicalize()
        .ok();
    if let Some(src) = ndi_src {
        if src.is_file() {
            let dst = dest_dir.join("Processing.NDI.Lib.x64.dll");
            fs::copy(&src, &dst).unwrap_or_else(|e| {
                panic!(
                    "momaku Windows build: failed to copy NDI DLL {} -> {}: {e}",
                    src.display(),
                    dst.display()
                )
            });
            println!("cargo:rerun-if-changed={}", src.display());
        }
    }
}

fn find_mozangle_out_dir(build_root: &Path) -> Option<PathBuf> {
    let read = fs::read_dir(build_root).ok()?;
    for entry in read.flatten() {
        let Ok(ft) = entry.file_type() else {
            continue;
        };
        if !ft.is_dir() {
            continue;
        }
        let name = entry.file_name();
        let n = name.to_string_lossy();
        if !n.starts_with("mozangle-") {
            continue;
        }
        let out = entry.path().join("out");
        if out.join("libEGL.dll").is_file() {
            return Some(out);
        }
    }
    None
}
