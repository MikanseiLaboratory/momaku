use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    // Run before `tauri_build` so `bundle.resources` paths exist when Tauri validates the merged config.
    copy_windows_runtime_dlls_next_to_exe();
    tauri_build::build();
}

/// surfman loads `libEGL.dll` from the DLL search path (typically the directory of `momaku.exe`).
/// mozangle builds those into its crate `OUT_DIR`; copy them next to the final `momaku.exe` output.
///
/// The destination directory is taken from **this package's** `OUT_DIR` (`…/target/…/<profile>/build/<pkg>/out`),
/// never from scanning other `target` trees — otherwise DLLs can land under the wrong `CARGO_TARGET_DIR`
/// while the exe is emitted next to the real build root.
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
    let out_dir = env::var("OUT_DIR").expect("OUT_DIR must be set by Cargo when build.rs runs");
    // Cargo layout: `$CARGO_TARGET_DIR/[<$triple>/]<profile>/build/<crate>-<hash>/out`
    let dest_dir = PathBuf::from(&out_dir)
        .ancestors()
        .nth(3)
        .unwrap_or_else(|| {
            panic!(
                "momaku Windows build: unexpected OUT_DIR (expected …/<profile>/build/<pkg>/out): {out_dir}"
            )
        })
        .to_path_buf();

    let build_root = dest_dir.join("build");
    let Some(mozangle_out_dir) = find_mozangle_out_dir(&build_root) else {
        panic!(
            "momaku Windows build: mozangle `out` with libEGL.dll not found under {build_root}. \
             Build the `servo` dependency first (Windows + `no-wgl` / mozangle `build_dlls`).",
            build_root = build_root.display(),
        );
    };

    for name in ["libEGL.dll", "libGLESv2.dll"] {
        let src = mozangle_out_dir.join(name);
        if !src.is_file() {
            panic!(
                "momaku Windows build: missing mozangle artifact {src}",
                src = src.display(),
            );
        }
        let dst = dest_dir.join(name);
        fs::copy(&src, &dst).unwrap_or_else(|e| {
            panic!(
                "momaku Windows build: failed to copy {src} -> {dst}: {e}",
                src = src.display(),
                dst = dst.display(),
            )
        });
        println!("cargo:rerun-if-changed={src}", src = src.display());
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
                    "momaku Windows build: failed to copy NDI DLL {src} -> {dst}: {e}",
                    src = src.display(),
                    dst = dst.display(),
                )
            });
            println!("cargo:rerun-if-changed={src}", src = src.display());
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
