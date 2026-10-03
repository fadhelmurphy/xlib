mod targets;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use console::{style, Term};
use dialoguer::{theme::ColorfulTheme, Confirm, MultiSelect};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use serde::{Deserialize, Serialize};

use targets::{Target, ALL};

#[derive(Parser)]
#[command(name = "xlib", version, about = "Tulis modul sekali di Rust, pakai di banyak bahasa")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Buat project modul baru
    Init { name: String },
    /// Build binding. Tanpa flag = menu interaktif.
    Build {
        /// Daftar target, dipisah koma (mis. python,go)
        #[arg(long, value_delimiter = ',')]
        target: Vec<String>,
        /// Build semua target yang siap
        #[arg(long)]
        all: bool,
        /// Abaikan cache, build ulang semua target terpilih
        #[arg(long)]
        force: bool,
        /// Folder project (default: folder saat ini)
        #[arg(long, default_value = ".")]
        path: PathBuf,
    },
    /// Cek toolchain tiap target
    Doctor,
}

#[derive(Deserialize)]
struct Config {
    package: Package,
    #[serde(default)]
    targets: BTreeMap<String, toml::Value>,
}

#[derive(Deserialize)]
struct Package {
    name: String,
    #[serde(default = "default_backend")]
    backend: String,
}

fn default_backend() -> String {
    "uniffi".into()
}

/// State lokal di `.xlib/`: pilihan menu terakhir + cache per target.
#[derive(Serialize, Deserialize, Default)]
struct State {
    last_targets: Vec<String>,
    cache: BTreeMap<String, String>,
}

impl State {
    fn load(project: &Path) -> Self {
        fs::read_to_string(project.join(".xlib/state.json"))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }
    fn save(&self, project: &Path) -> Result<()> {
        fs::create_dir_all(project.join(".xlib"))?;
        fs::write(project.join(".xlib/state.json"), serde_json::to_string_pretty(self)?)?;
        Ok(())
    }
}

fn main() {
    if let Err(e) = run() {
        eprintln!("{} {e:#}", style("✖").red().bold());
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    match Cli::parse().cmd {
        Cmd::Init { name } => init(&name),
        Cmd::Build { target, all, force, path } => build(&path, target, all, force),
        Cmd::Doctor => {
            doctor();
            Ok(())
        }
    }
}

// ---------------------------------------------------------------- init

fn init(name: &str) -> Result<()> {
    let dir = Path::new(name);
    if dir.exists() {
        bail!("folder `{name}` sudah ada");
    }
    let lib_name = name.replace('-', "_");
    let fill = |s: &str| s.replace("{{name}}", name).replace("{{lib_name}}", &lib_name);

    fs::create_dir_all(dir.join("src"))?;
    fs::write(dir.join("Cargo.toml"), fill(include_str!("../templates/Cargo.toml.tmpl")))?;
    fs::write(dir.join("xlib.toml"), fill(include_str!("../templates/xlib.toml.tmpl")))?;
    fs::write(dir.join("uniffi-bindgen.rs"), include_str!("../templates/uniffi-bindgen.rs"))?;
    fs::write(dir.join("src/lib.rs"), include_str!("../templates/src/lib.rs"))?;
    fs::write(dir.join(".gitignore"), "/target\n/dist\n/.xlib\n")?;

    println!("{} project `{name}` dibuat", style("✔").green());
    println!("  cd {name} && xlib build");
    Ok(())
}

// ---------------------------------------------------------------- doctor

fn doctor() {
    println!("{}", style("Toolchain").bold());
    let ok = |b: bool| if b { style("✔").green() } else { style("✖").red() };
    println!("  {} cargo", ok(targets::on_path("cargo")));
    for t in ALL {
        match targets::check(t) {
            None => println!("  {} {:<11} siap", ok(true), t.label),
            Some(w) => println!("  {} {:<11} {w}", if targets::blocking(t) { ok(false) } else { style("⚠").yellow() }, t.label),
        }
    }
    if !targets::on_path("uniffi-bindgen-go") {
        println!("\n  Pasang generator Go:");
        println!("  cargo install uniffi-bindgen-go --git https://github.com/NordSecurity/uniffi-bindgen-go");
    }
}

// ---------------------------------------------------------------- build

fn build(project: &Path, flags: Vec<String>, all: bool, force: bool) -> Result<()> {
    let project = project.canonicalize().context("folder project tidak ditemukan")?;
    let cfg: Config = toml::from_str(
        &fs::read_to_string(project.join("xlib.toml")).context("xlib.toml tidak ditemukan, jalankan `xlib init` dulu")?,
    )?;
    if cfg.package.backend != "uniffi" {
        bail!("backend `{}` belum didukung (saat ini: uniffi)", cfg.package.backend);
    }
    let mut state = State::load(&project);

    // 1. Tentukan target: flag > menu (kalau TTY) > xlib.toml
    let selected: Vec<&Target> = if all {
        ALL.iter().filter(|t| !targets::blocking(t)).collect()
    } else if !flags.is_empty() {
        flags
            .iter()
            .map(|id| targets::find(id).with_context(|| format!("target tidak dikenal: {id}")))
            .collect::<Result<_>>()?
    } else if Term::stderr().is_term() {
        menu(&cfg, &state)?
    } else {
        cfg.targets.keys().filter_map(|id| targets::find(id)).collect()
    };
    if selected.is_empty() {
        println!("Tidak ada target dipilih.");
        return Ok(());
    }
    state.last_targets = selected.iter().map(|t| t.id.to_string()).collect();

    // 2. Compile core Rust sekali untuk semua target
    let lib_name = cfg.package.name.replace('-', "_");
    let sp = spinner(None, "Compile Rust core");
    let t0 = Instant::now();
    let out = Command::new("cargo")
        .current_dir(&project)
        .args(["build", "--release", "--lib", "--bin", "uniffi-bindgen"])
        .output()?;
    if !out.status.success() {
        sp.finish_and_clear();
        bail!("compile gagal:\n{}", String::from_utf8_lossy(&out.stderr));
    }
    done(&sp, format!("{} Rust core  {}", style("✔").green(), secs(t0)));

    let release = project.join("target/release");
    let lib_path = release.join(targets::dylib_name(&lib_name));
    let bindgen = release.join("uniffi-bindgen");
    let fingerprint = fingerprint(&lib_path)?;

    // 3. Generate binding per target secara paralel
    let mp = Arc::new(MultiProgress::new());
    let handles: Vec<_> = selected
        .iter()
        .map(|t| {
            let (t, mp) = (*t, mp.clone());
            let (project, lib_path, bindgen, name) = (project.clone(), lib_path.clone(), bindgen.clone(), cfg.package.name.clone());
            let cached = !force && state.cache.get(t.id) == Some(&fingerprint) && project.join("dist").join(t.id).exists();
            std::thread::spawn(move || {
                let sp = spinner(Some(&mp), t.label);
                let t0 = Instant::now();
                if cached {
                    done(&sp, format!("{} {:<11} cache (tidak berubah)", style("✔").green(), t.label));
                    return (t.id, true);
                }
                if targets::blocking(t) {
                    done(&sp, format!("{} {:<11} dilewati: {}", style("⚠").yellow(), t.label, targets::check(t).unwrap()));
                    return (t.id, false);
                }
                let out_dir = project.join("dist").join(t.id);
                let res = fs::remove_dir_all(&out_dir)
                    .or_else(|_| Ok::<_, std::io::Error>(()))
                    .map_err(anyhow::Error::from)
                    .and_then(|_| fs::create_dir_all(&out_dir).map_err(Into::into))
                    .and_then(|_| targets::generate(t, &bindgen, &lib_path, &out_dir))
                    .and_then(|_| package(t, &name, &lib_path, &out_dir));
                match res {
                    Ok(()) => {
                        done(&sp, format!("{} {:<11} {}  → dist/{}", style("✔").green(), t.label, secs(t0), t.id));
                        (t.id, true)
                    }
                    Err(e) => {
                        done(&sp, format!("{} {:<11} gagal: {e}", style("✖").red(), t.label));
                        (t.id, false)
                    }
                }
            })
        })
        .collect();

    let mut failed = 0;
    for h in handles {
        let (id, ok) = h.join().expect("thread panic");
        if ok {
            state.cache.insert(id.to_string(), fingerprint.clone());
        } else {
            state.cache.remove(id);
            failed += 1;
        }
    }
    state.save(&project)?;

    if failed > 0 {
        bail!("{failed} target gagal/dilewati");
    }
    Ok(())
}

fn menu<'a>(cfg: &Config, state: &State) -> Result<Vec<&'a Target>> {
    let defaults: Vec<String> = if state.last_targets.is_empty() {
        cfg.targets.keys().cloned().collect()
    } else {
        state.last_targets.clone()
    };
    let items: Vec<String> = ALL
        .iter()
        .map(|t| {
            let note = targets::check(t).map(|w| format!("  ⚠ {w}")).unwrap_or_default();
            format!("{:<11} ({}){}", t.label, t.output, note)
        })
        .collect();
    let checked: Vec<bool> = ALL
        .iter()
        .map(|t| defaults.iter().any(|d| d == t.id) && !targets::blocking(t))
        .collect();

    let picks = MultiSelect::with_theme(&ColorfulTheme::default())
        .with_prompt("Pilih target build  (spasi = pilih, a = semua, enter = lanjut)")
        .items(&items)
        .defaults(&checked)
        .interact()?;
    let chosen: Vec<&Target> = picks.into_iter().map(|i| &ALL[i]).collect();

    let blocked: Vec<_> = chosen.iter().filter(|t| targets::blocking(t)).map(|t| t.label).collect();
    if !blocked.is_empty()
        && !Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt(format!("{} belum siap dan akan dilewati. Lanjut?", blocked.join(", ")))
            .default(true)
            .interact()?
    {
        bail!("dibatalkan");
    }
    Ok(chosen)
}

/// Rapikan output tiap target jadi sesuatu yang langsung bisa dipakai.
fn package(t: &Target, name: &str, lib_path: &Path, out: &Path) -> Result<()> {
    let lib_file = lib_path.file_name().unwrap();
    let module = name.replace('-', "_");
    match t.id {
        "python" => {
            // dist/python/<module>/{__init__.py, <module>.py, lib<module>.so} + pyproject.toml
            let pkg = out.join(&module);
            fs::create_dir_all(&pkg)?;
            fs::rename(out.join(format!("{module}.py")), pkg.join(format!("{module}.py")))?;
            fs::copy(lib_path, pkg.join(lib_file))?;
            fs::write(pkg.join("__init__.py"), format!("from .{module} import *  # noqa\n"))?;
            fs::write(
                out.join("pyproject.toml"),
                format!(
                    "[build-system]\nrequires = [\"setuptools>=61\"]\nbuild-backend = \"setuptools.build_meta\"\n\n\
                     [project]\nname = \"{name}\"\nversion = \"0.1.0\"\n\n\
                     [tool.setuptools]\npackages = [\"{module}\"]\n\n\
                     [tool.setuptools.package-data]\n{module} = [\"*.so\", \"*.dylib\", \"*.dll\"]\n"
                ),
            )?;
        }
        _ => {
            fs::copy(lib_path, out.join(lib_file))?;
        }
    }
    let hint = match t.id {
        "python" => format!("pip install ./dist/python\npython -c \"import {module}; print({module}.add(2, 3))\""),
        "kotlin" => format!("Salin file .kt ke project, tambahkan dependency JNA (net.java.dev.jna:jna),\nlalu taruh {} di jna.library.path / jniLibs.", lib_file.to_string_lossy()),
        "swift" => "Tambahkan .swift + header + modulemap ke target Xcode, link library-nya.\n(Packaging XCFramework otomatis menyusul, butuh macOS.)".into(),
        "go" => format!("Import package hasil generate, set CGO_LDFLAGS=\"-L<folder ini> -l{module}\"\ndan LD_LIBRARY_PATH ke folder ini saat run."),
        _ => String::new(),
    };
    fs::write(out.join("CARA_PAKAI.txt"), hint + "\n")?;
    Ok(())
}

// ---------------------------------------------------------------- util

fn spinner(mp: Option<&MultiProgress>, label: &str) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    let pb = match mp {
        Some(mp) => mp.add(pb),
        None => pb,
    };
    pb.set_style(ProgressStyle::with_template("{spinner:.cyan} {msg}").unwrap());
    pb.set_message(format!("{label} ..."));
    pb.enable_steady_tick(Duration::from_millis(80));
    pb
}

/// Selesaikan spinner; kalau bukan terminal (CI), cetak sebagai baris biasa.
fn done(pb: &ProgressBar, msg: String) {
    if pb.is_hidden() {
        println!("{msg}");
    }
    pb.finish_with_message(msg);
}

fn secs(t0: Instant) -> String {
    format!("{:.1}s", t0.elapsed().as_secs_f32())
}

fn fingerprint(p: &Path) -> Result<String> {
    let m = fs::metadata(p).with_context(|| format!("library tidak ditemukan: {}", p.display()))?;
    let mtime = m.modified()?.duration_since(UNIX_EPOCH)?.as_nanos();
    Ok(format!("{}-{mtime}", m.len()))
}
