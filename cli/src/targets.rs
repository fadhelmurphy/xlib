//! Definisi target bahasa. Menambah bahasa baru = menambah satu entri di `ALL`.

use std::path::Path;
use std::process::Command;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Generator {
    /// Generator bawaan UniFFI (`uniffi-bindgen generate --language <x>`).
    Uniffi(&'static str),
    /// Generator eksternal (binary terpisah, mis. uniffi-bindgen-go).
    External(&'static str),
    /// Belum didukung di prototipe ini.
    Planned,
}

pub struct Target {
    pub id: &'static str,
    pub label: &'static str,
    pub output: &'static str,
    pub generator: Generator,
}

pub const ALL: &[Target] = &[
    Target { id: "python", label: "Python", output: "pip package", generator: Generator::Uniffi("python") },
    Target { id: "kotlin", label: "Kotlin", output: "JVM / Android", generator: Generator::Uniffi("kotlin") },
    Target { id: "swift", label: "Swift", output: "iOS / macOS", generator: Generator::Uniffi("swift") },
    Target { id: "go", label: "Go", output: "Go module (cgo)", generator: Generator::External("uniffi-bindgen-go") },
    Target { id: "js", label: "JavaScript", output: "Node / browser", generator: Generator::Planned },
];

pub fn find(id: &str) -> Option<&'static Target> {
    ALL.iter().find(|t| t.id == id)
}

/// Peringatan kalau toolchain target belum siap. `None` = siap.
pub fn check(t: &Target) -> Option<String> {
    match t.generator {
        Generator::Planned => Some("belum didukung di prototipe".into()),
        Generator::External(bin) if !on_path(bin) => Some(format!("butuh `{bin}` (jalankan `xlib doctor`)")),
        _ if t.id == "swift" && !cfg!(target_os = "macos") => {
            Some("binding bisa dibuat, tapi build iOS butuh macOS + Xcode".into())
        }
        _ => None,
    }
}

/// Peringatan yang menghalangi build (bukan sekadar catatan).
pub fn blocking(t: &Target) -> bool {
    matches!(t.generator, Generator::Planned)
        || matches!(t.generator, Generator::External(bin) if !on_path(bin))
}

pub fn on_path(bin: &str) -> bool {
    Command::new(bin).arg("--help").output().is_ok()
}

/// Nama file library dinamis sesuai OS.
pub fn dylib_name(lib: &str) -> String {
    if cfg!(target_os = "macos") {
        format!("lib{lib}.dylib")
    } else if cfg!(target_os = "windows") {
        format!("{lib}.dll")
    } else {
        format!("lib{lib}.so")
    }
}

/// Generate binding untuk satu target ke `out_dir`.
pub fn generate(t: &Target, bindgen: &Path, lib_path: &Path, out_dir: &Path) -> anyhow::Result<()> {
    // bindgen ada di <crate>/target/release/; uniffi butuh cwd = folder crate.
    let crate_dir = bindgen.ancestors().nth(3).unwrap_or(Path::new("."));
    let output = match t.generator {
        Generator::Uniffi(lang) => Command::new(bindgen)
            .current_dir(crate_dir)
            .args(["generate", "--library"])
            .arg(lib_path)
            .args(["--language", lang, "--out-dir"])
            .arg(out_dir)
            .output()?,
        Generator::External(bin) => Command::new(bin)
            .arg("--library")
            .arg(lib_path)
            .arg("--out-dir")
            .arg(out_dir)
            .output()?,
        Generator::Planned => anyhow::bail!("target belum didukung"),
    };
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        let first = err.lines().find(|l| !l.trim().is_empty()).unwrap_or("error tidak diketahui");
        anyhow::bail!("{}", first.trim_start_matches("Error: "));
    }
    Ok(())
}
