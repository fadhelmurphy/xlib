//! Bahasa sumber selain Rust (C++, Go).
//!
//! Alurnya sama untuk semua bahasa:
//!   1. parse kode sumber jadi daftar fungsi (IR)
//!   2. generate wrapper C ABI di bahasa itu (extern "C" / //export)
//!   3. generate crate Rust tipis (shim) yang memanggil C ABI tadi dan
//!      mengekspornya lewat UniFFI
//!   4. shim itu masuk ke pipeline build yang sama dengan modul Rust biasa
//!
//! Jadi semua target (Python, Kotlin, Swift, Go, ...) otomatis ikut.

pub mod cpp;
pub mod go;

use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use anyhow::Result;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ty {
    Void,
    Bool,
    I32,
    I64,
    F64,
    Str,
    VecF64,
    VecI64,
}

impl Ty {
    pub fn rust(self) -> &'static str {
        match self {
            Ty::Void => "()",
            Ty::Bool => "bool",
            Ty::I32 => "i32",
            Ty::I64 => "i64",
            Ty::F64 => "f64",
            Ty::Str => "String",
            Ty::VecF64 => "Vec<f64>",
            Ty::VecI64 => "Vec<i64>",
        }
    }

    /// Tipe di sisi C ABI untuk nilai balik (dilihat dari Rust).
    fn rust_ffi_ret(self) -> &'static str {
        match self {
            Ty::Void => "()",
            Ty::Bool => "bool",
            Ty::I32 => "i32",
            Ty::I64 => "i64",
            Ty::F64 => "f64",
            Ty::Str => "*mut c_char",
            Ty::VecF64 => "*mut f64",
            Ty::VecI64 => "*mut i64",
        }
    }

    pub fn is_vec(self) -> bool {
        matches!(self, Ty::VecF64 | Ty::VecI64)
    }
}

#[derive(Clone, Debug)]
pub struct Param {
    pub name: String,
    pub ty: Ty,
    /// Tipe asli di bahasa sumber (dipakai untuk konversi di wrapper).
    pub raw: String,
}

#[derive(Clone, Debug)]
pub struct Func {
    /// Nama di bahasa sumber (mis. `Add` di Go, `add` di C++).
    pub source_name: String,
    /// Nama yang diekspor ke semua bahasa target (snake_case).
    pub name: String,
    pub params: Vec<Param>,
    pub ret: Ty,
    /// Bisa gagal: C++ tanpa `noexcept`, atau Go yang mengembalikan `error`.
    pub fallible: bool,
}

impl Func {
    pub fn c_symbol(&self) -> String {
        format!("xlib_{}", self.name)
    }
}

/// Fungsi yang dilewati karena tipenya belum didukung.
#[derive(Debug)]
pub struct Skipped {
    pub name: String,
    pub reason: String,
}

pub struct Parsed {
    pub funcs: Vec<Func>,
    pub skipped: Vec<Skipped>,
}

pub fn snake_case(s: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = s.chars().collect();
    for (i, c) in chars.iter().enumerate() {
        if c.is_uppercase() {
            let prev_lower = i > 0 && (chars[i - 1].is_lowercase() || chars[i - 1].is_ascii_digit());
            let next_lower = chars.get(i + 1).is_some_and(|n| n.is_lowercase());
            if i > 0 && (prev_lower || (next_lower && chars[i - 1].is_uppercase())) {
                out.push('_');
            }
            out.extend(c.to_lowercase());
        } else {
            out.push(*c);
        }
    }
    out
}

/// Nama parameter yang aman dipakai di Rust.
pub fn safe_ident(name: &str) -> String {
    const KW: &[&str] = &[
        "as", "break", "const", "continue", "crate", "else", "enum", "extern", "false", "fn", "for", "if", "impl",
        "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return", "self", "static", "struct",
        "super", "trait", "true", "type", "unsafe", "use", "where", "while", "async", "await", "dyn", "err",
    ];
    if KW.contains(&name) {
        format!("{name}_")
    } else {
        name.to_string()
    }
}

/// Tulis crate Rust shim ke `dir`. `build_rs` diisi oleh tiap bahasa sumber
/// (compile C++ via `cc`, atau link archive hasil `go build`).
pub fn write_shim(dir: &Path, package: &str, funcs: &[Func], build_rs: &str, build_deps: &str) -> Result<()> {
    let lib = package.replace('-', "_");
    fs::create_dir_all(dir.join("src"))?;

    write_if_changed(
        &dir.join("Cargo.toml"),
        &format!(
            r#"# Dibuat otomatis oleh xlib. Jangan diedit.
[package]
name = "{package}"
version = "0.1.0"
edition = "2021"
build = "build.rs"

[lib]
crate-type = ["cdylib", "staticlib", "lib"]
name = "{lib}"

[[bin]]
name = "uniffi-bindgen"
path = "uniffi-bindgen.rs"

[dependencies]
uniffi = {{ version = "=0.31", features = ["cli"] }}

[build-dependencies]
{build_deps}

[profile.release]
opt-level = 3

[workspace]
"#
        ),
    )?;
    write_if_changed(&dir.join("uniffi-bindgen.rs"), "fn main() { uniffi::uniffi_bindgen_main() }\n")?;
    write_if_changed(&dir.join("build.rs"), build_rs)?;
    write_if_changed(&dir.join("src/lib.rs"), &shim_lib_rs(funcs))?;
    Ok(())
}

/// Hindari menyentuh file yang isinya sama, supaya cargo tidak compile ulang.
pub fn write_if_changed(path: &Path, content: &str) -> Result<()> {
    if fs::read_to_string(path).ok().as_deref() != Some(content) {
        fs::write(path, content)?;
    }
    Ok(())
}

fn shim_lib_rs(funcs: &[Func]) -> String {
    let mut s = String::from(
        r#"// Dibuat otomatis oleh xlib dari kode sumber modul. Jangan diedit.
#![allow(clippy::all, dead_code, unused_unsafe, non_snake_case)]

use std::ffi::{c_char, c_void, CStr, CString};

uniffi::setup_scaffolding!();

#[repr(C)]
struct XlibErr {
    msg: *mut c_char,
}

/// Error yang dilempar modul (exception C++ / `error` Go).
#[derive(Debug, uniffi::Error)]
#[uniffi(flat_error)]
pub enum XlibError {
    Failed(String),
}

impl std::fmt::Display for XlibError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            XlibError::Failed(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for XlibError {}

extern "C" {
    fn free(p: *mut c_void);
}

unsafe fn take_err(e: XlibErr) -> Result<(), XlibError> {
    if e.msg.is_null() {
        return Ok(());
    }
    let message = CStr::from_ptr(e.msg).to_string_lossy().into_owned();
    free(e.msg as *mut c_void);
    Err(XlibError::Failed(message))
}

unsafe fn take_str(p: *mut c_char) -> String {
    if p.is_null() {
        return String::new();
    }
    let s = CStr::from_ptr(p).to_string_lossy().into_owned();
    free(p as *mut c_void);
    s
}

unsafe fn take_vec<T: Copy>(p: *mut T, n: usize) -> Vec<T> {
    if p.is_null() {
        return Vec::new();
    }
    let v = std::slice::from_raw_parts(p, n).to_vec();
    free(p as *mut c_void);
    v
}

fn c_string(s: String) -> CString {
    CString::new(s).expect("string tidak boleh mengandung karakter NUL")
}

"#,
    );

    // Deklarasi extern
    s.push_str("extern \"C\" {\n");
    for f in funcs {
        let mut args: Vec<String> = Vec::new();
        for p in &f.params {
            let n = safe_ident(&p.name);
            match p.ty {
                Ty::Str => args.push(format!("{n}: *const c_char")),
                Ty::VecF64 => args.extend([format!("{n}_ptr: *const f64"), format!("{n}_len: usize")]),
                Ty::VecI64 => args.extend([format!("{n}_ptr: *const i64"), format!("{n}_len: usize")]),
                t => args.push(format!("{n}: {}", t.rust())),
            }
        }
        if f.ret.is_vec() {
            args.push("out_len: *mut usize".into());
        }
        args.push("err: *mut XlibErr".into());
        let ret = if f.ret == Ty::Void { String::new() } else { format!(" -> {}", f.ret.rust_ffi_ret()) };
        let _ = writeln!(s, "    fn {}({}){};", f.c_symbol(), args.join(", "), ret);
    }
    s.push_str("}\n");

    // Fungsi yang diekspor
    for f in funcs {
        let params: Vec<String> = f.params.iter().map(|p| format!("{}: {}", safe_ident(&p.name), p.ty.rust())).collect();
        let ret_ty = if f.fallible { format!("Result<{}, XlibError>", f.ret.rust()) } else { f.ret.rust().to_string() };
        let _ = writeln!(s, "\n#[uniffi::export]\npub fn {}({}) -> {} {{", f.name, params.join(", "), ret_ty);
        s.push_str("    unsafe {\n");
        let mut call_args: Vec<String> = Vec::new();
        for p in &f.params {
            let n = safe_ident(&p.name);
            match p.ty {
                Ty::Str => {
                    let _ = writeln!(s, "        let {n}_c = c_string({n});");
                    call_args.push(format!("{n}_c.as_ptr()"));
                }
                Ty::VecF64 | Ty::VecI64 => call_args.extend([format!("{n}.as_ptr()"), format!("{n}.len()")]),
                _ => call_args.push(n),
            }
        }
        if f.ret.is_vec() {
            s.push_str("        let mut out_len: usize = 0;\n");
            call_args.push("&mut out_len".into());
        }
        s.push_str("        let mut err = XlibErr { msg: std::ptr::null_mut() };\n");
        call_args.push("&mut err".into());
        let _ = writeln!(s, "        let raw = {}({});", f.c_symbol(), call_args.join(", "));
        if f.fallible {
            s.push_str("        take_err(err)?;\n");
        } else {
            s.push_str("        if let Err(e) = take_err(err) { panic!(\"{e}\"); }\n");
        }
        let value = match f.ret {
            Ty::Str => "take_str(raw)",
            Ty::VecF64 | Ty::VecI64 => "take_vec(raw, out_len)",
            _ => "raw",
        };
        if f.fallible {
            let _ = writeln!(s, "        Ok({value})");
        } else {
            let _ = writeln!(s, "        {value}");
        }
        s.push_str("    }\n}\n");
    }
    s
}
