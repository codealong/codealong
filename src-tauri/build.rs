fn main() {
    engine_id();
    let mut attributes = tauri_build::Attributes::new();
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        attributes = attributes.windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest());
        common_controls();
    }
    // As tauri_build::build() does with its error, which takes the default attributes.
    if let Err(error) = tauri_build::try_build(attributes) {
        println!("{error:#}");
        std::process::exit(1);
    }
}

/// Windows: a manifest asking for Common Controls 6, the comctl32.dll that has TaskDialogIndirect,
/// which the binary imports (Tauri's dialogs); Windows won't start a binary without it
/// (STATUS_ENTRYPOINT_NOT_FOUND). Tauri embeds that manifest in the app alone, so the linker
/// writes it into every binary of this crate instead, the tests' too.
fn common_controls() {
    println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
    println!(
        "cargo:rustc-link-arg=/MANIFESTDEPENDENCY:type='win32' name='Microsoft.Windows.Common-Controls' version='6.0.0.0' processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'"
    );
}

/// `CODEALONG_ENGINE`: a hash of what the engine is built from, the Rust of this crate and
/// codealong-peer and what they're built with, but not the windows' pages, which the binary
/// carries too. An app keeps the server it finds when this is the same as its own (serve.rs), so
/// a build that changed only the windows leaves the agents the server runs going. The hasher's
/// keys are fixed, so the same files hash the same in every build by the same compiler; a build by
/// another is taken for another engine, which it may well be.
fn engine_id() {
    use std::hash::{Hash, Hasher};
    use std::path::{Path, PathBuf};

    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else {
                out.push(path);
            }
        }
    }

    let mut files = Vec::new();
    for dir in ["src", "peer/src"] {
        println!("cargo:rerun-if-changed={dir}");
        walk(Path::new(dir), &mut files);
    }
    for file in ["Cargo.toml", "Cargo.lock", "peer/Cargo.toml", "build.rs"] {
        println!("cargo:rerun-if-changed={file}");
        files.push(PathBuf::from(file));
    }
    // Another checkout building into the shared folder is another engine (lib.rs `CHECKOUT`).
    println!("cargo:rerun-if-env-changed=CODEALONG_CHECKOUT");
    files.sort();
    #[allow(deprecated)]
    let mut h = std::hash::SipHasher::new();
    for file in &files {
        file.to_string_lossy().hash(&mut h);
        std::fs::read(file).unwrap_or_default().hash(&mut h);
    }
    println!("cargo:rustc-env=CODEALONG_ENGINE={:016x}", h.finish());
}
