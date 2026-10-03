//! Sumber Go: fungsi yang diekspor (huruf kapital, tanpa receiver) di package
//! pada root project (ada go.mod).
//!
//! Tipe yang didukung: bool, int, int32, int64, float64, string, []float64,
//! []int64. Fungsi yang mengembalikan `error` (atau `(T, error)`) diteruskan
//! sebagai error ke bahasa target. Panic juga ditangkap.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use regex::Regex;

use super::{snake_case, write_if_changed, Func, Param, Parsed, Skipped, Ty};

fn go_files(project: &Path) -> Result<Vec<PathBuf>> {
    let mut v: Vec<PathBuf> = fs::read_dir(project)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            let n = p.file_name().unwrap().to_string_lossy();
            n.ends_with(".go") && !n.ends_with("_test.go")
        })
        .collect();
    v.sort();
    if v.is_empty() {
        bail!("tidak ada file .go di root project");
    }
    Ok(v)
}

fn map_type(t: &str) -> Option<Ty> {
    Some(match t.trim() {
        "bool" => Ty::Bool,
        "int32" => Ty::I32,
        "int" | "int64" => Ty::I64,
        "float64" => Ty::F64,
        "string" => Ty::Str,
        "[]float64" => Ty::VecF64,
        "[]int64" => Ty::VecI64,
        _ => return None,
    })
}

pub fn parse(project: &Path) -> Result<Parsed> {
    let fn_re = Regex::new(r"(?m)^func\s+([A-Z]\w*)\s*\(([^()]*)\)\s*(\([^()]*\)|[\w\[\]\.\*]+)?\s*\{").unwrap();
    let mut funcs = Vec::new();
    let mut skipped = Vec::new();
    for file in go_files(project)? {
        let text = fs::read_to_string(&file)?;
        for c in fn_re.captures_iter(&text) {
            let name = c[1].to_string();
            match build_func(&name, &c[2], c.get(3).map_or("", |m| m.as_str())) {
                Ok(f) => funcs.push(f),
                Err(reason) => skipped.push(Skipped { name, reason }),
            }
        }
    }
    Ok(Parsed { funcs, skipped })
}

fn build_func(name: &str, params: &str, ret: &str) -> std::result::Result<Func, String> {
    // Parameter Go bisa dikelompokkan: `a, b int64, s string`
    let mut ps: Vec<Param> = Vec::new();
    let mut pending: Vec<String> = Vec::new();
    for part in params.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        let mut it = part.splitn(2, char::is_whitespace);
        let pname = it.next().unwrap().to_string();
        match it.next() {
            None => pending.push(pname),
            Some(raw) => {
                let ty = map_type(raw).ok_or_else(|| format!("parameter `{part}` belum didukung"))?;
                for n in pending.drain(..).chain(std::iter::once(pname)) {
                    ps.push(Param { name: n, ty, raw: raw.trim().to_string() });
                }
            }
        }
    }
    if !pending.is_empty() {
        return Err("parameter tanpa nama belum didukung".into());
    }

    let ret = ret.trim().trim_start_matches('(').trim_end_matches(')');
    let outs: Vec<&str> = ret.split(',').map(str::trim).filter(|s| !s.is_empty()).collect();
    let (ret_ty, fallible) = match outs.as_slice() {
        [] => (Ty::Void, false),
        ["error"] => (Ty::Void, true),
        [t] => (map_type(t).ok_or_else(|| format!("tipe balik `{t}` belum didukung"))?, false),
        [t, "error"] => (map_type(t).ok_or_else(|| format!("tipe balik `{t}` belum didukung"))?, true),
        _ => return Err(format!("tipe balik `({ret})` belum didukung")),
    };
    Ok(Func { source_name: name.to_string(), name: snake_case(name), params: ps, ret: ret_ty, fallible })
}

/// Siapkan folder kerja Go: salin kode modul (package diganti `main`, syarat
/// `-buildmode=c-archive`) + file wrapper `//export`, lalu build archive.
pub fn build_archive(project: &Path, work: &Path, funcs: &[Func]) -> Result<PathBuf> {
    fs::create_dir_all(work)?;
    let pkg_re = Regex::new(r"(?m)^package\s+\w+").unwrap();
    for f in ["go.mod", "go.sum"] {
        if project.join(f).exists() {
            write_if_changed(&work.join(f), &fs::read_to_string(project.join(f))?)?;
        }
    }
    for file in go_files(project)? {
        let text = fs::read_to_string(&file)?;
        write_if_changed(&work.join(file.file_name().unwrap()), &pkg_re.replace(&text, "package main"))?;
    }
    write_if_changed(&work.join("zz_xlib_export.go"), &wrapper(funcs))?;

    let out = work.join("out");
    fs::create_dir_all(&out)?;
    let archive = out.join("libxlib_go.a");
    let fresh = work.join("libxlib_go.new.a");
    let res = Command::new("go")
        .current_dir(work)
        .env("CGO_ENABLED", "1")
        .args(["build", "-trimpath", "-buildmode=c-archive", "-o"])
        .arg(&fresh)
        .arg(".")
        .output()
        .context("`go` tidak ditemukan")?;
    if !res.status.success() {
        bail!("go build gagal:\n{}", String::from_utf8_lossy(&res.stderr));
    }
    // Ganti archive hanya kalau isinya berubah, supaya cargo & cache xlib tidak build ulang.
    if fs::read(&archive).ok() != fs::read(&fresh).ok() {
        fs::rename(&fresh, &archive)?;
    } else {
        fs::remove_file(&fresh)?;
    }
    let _ = fs::remove_file(work.join("libxlib_go.new.h"));
    Ok(out)
}

fn wrapper(funcs: &[Func]) -> String {
    let mut s = String::from(
        r#"// Dibuat otomatis oleh xlib. Jangan diedit.
package main

/*
#include <stdint.h>
#include <stdlib.h>
#include <stdbool.h>
#include <string.h>
typedef struct { char* msg; } XlibErr;
*/
import "C"

import (
	"fmt"
	"unsafe"
)

var _ = unsafe.Pointer(nil)

func main() {}

func xlibErr(err *C.XlibErr, msg string) { err.msg = C.CString(msg) }

func xlibF64s(p *C.double, n C.size_t) []float64 {
	if n == 0 {
		return []float64{}
	}
	return append([]float64(nil), unsafe.Slice((*float64)(unsafe.Pointer(p)), int(n))...)
}

func xlibI64s(p *C.int64_t, n C.size_t) []int64 {
	if n == 0 {
		return []int64{}
	}
	return append([]int64(nil), unsafe.Slice((*int64)(unsafe.Pointer(p)), int(n))...)
}

func xlibBuf[T any](v []T, outLen *C.size_t) unsafe.Pointer {
	*outLen = C.size_t(len(v))
	if len(v) == 0 {
		return nil
	}
	size := C.size_t(len(v)) * C.size_t(unsafe.Sizeof(v[0]))
	p := C.malloc(size)
	C.memcpy(p, unsafe.Pointer(&v[0]), size)
	return p
}
"#,
    );

    for f in funcs {
        let mut args = Vec::new();
        let mut call = Vec::new();
        for (i, p) in f.params.iter().enumerate() {
            let n = format!("a{i}");
            match p.ty {
                Ty::Bool => {
                    args.push(format!("{n} C.bool"));
                    call.push(format!("bool({n})"));
                }
                Ty::I32 => {
                    args.push(format!("{n} C.int32_t"));
                    call.push(format!("int32({n})"));
                }
                Ty::I64 => {
                    args.push(format!("{n} C.int64_t"));
                    // Go `int` dan `int64` sama-sama jadi i64; konversi ke tipe aslinya.
                    call.push(format!("{}({n})", p.raw));
                }
                Ty::F64 => {
                    args.push(format!("{n} C.double"));
                    call.push(format!("float64({n})"));
                }
                Ty::Str => {
                    args.push(format!("{n} *C.char"));
                    call.push(format!("C.GoString({n})"));
                }
                Ty::VecF64 => {
                    args.push(format!("{n} *C.double, {n}Len C.size_t"));
                    call.push(format!("xlibF64s({n}, {n}Len)"));
                }
                Ty::VecI64 => {
                    args.push(format!("{n} *C.int64_t, {n}Len C.size_t"));
                    call.push(format!("xlibI64s({n}, {n}Len)"));
                }
                Ty::Void => unreachable!(),
            }
        }
        if f.ret.is_vec() {
            args.push("outLen *C.size_t".into());
        }
        args.push("err *C.XlibErr".into());

        let ret_c = match f.ret {
            Ty::Void => "",
            Ty::Bool => "(ret C.bool)",
            Ty::I32 => "(ret C.int32_t)",
            Ty::I64 => "(ret C.int64_t)",
            Ty::F64 => "(ret C.double)",
            Ty::Str => "(ret *C.char)",
            Ty::VecF64 => "(ret *C.double)",
            Ty::VecI64 => "(ret *C.int64_t)",
        };
        let invoke = format!("{}({})", f.source_name, call.join(", "));
        let _ = writeln!(s, "\n//export {}\nfunc {}({}) {} {{", f.c_symbol(), f.c_symbol(), args.join(", "), ret_c);
        s.push_str("\tdefer func() {\n\t\tif r := recover(); r != nil {\n\t\t\txlibErr(err, fmt.Sprint(\"panic: \", r))\n\t\t}\n\t}()\n");
        let lhs = match (f.ret, f.fallible) {
            (Ty::Void, false) => String::new(),
            (Ty::Void, true) => "e := ".into(),
            (_, false) => "v := ".into(),
            (_, true) => "v, e := ".into(),
        };
        let _ = writeln!(s, "\t{lhs}{invoke}");
        if f.fallible {
            s.push_str("\tif e != nil {\n\t\txlibErr(err, e.Error())\n\t\treturn\n\t}\n");
        }
        let conv = match f.ret {
            Ty::Void => None,
            Ty::Bool => Some("C.bool(v)".to_string()),
            Ty::I32 => Some("C.int32_t(v)".into()),
            Ty::I64 => Some("C.int64_t(v)".into()),
            Ty::F64 => Some("C.double(v)".into()),
            Ty::Str => Some("C.CString(v)".into()),
            Ty::VecF64 => Some("(*C.double)(xlibBuf(v, outLen))".into()),
            Ty::VecI64 => Some("(*C.int64_t)(xlibBuf(v, outLen))".into()),
        };
        if let Some(c) = conv {
            let _ = writeln!(s, "\treturn {c}");
        } else {
            s.push_str("\treturn\n");
        }
        s.push_str("}\n");
    }
    s
}

pub fn build_rs(archive_dir: &Path) -> String {
    format!(
        r#"// Dibuat otomatis oleh xlib. Link archive Go hasil `go build -buildmode=c-archive`.
fn main() {{
    println!("cargo:rustc-link-search=native={dir}");
    println!("cargo:rustc-link-lib=static=xlib_go");
    if cfg!(target_os = "macos") {{
        println!("cargo:rustc-link-lib=framework=CoreFoundation");
        println!("cargo:rustc-link-lib=framework=Security");
    }} else {{
        println!("cargo:rustc-link-lib=pthread");
    }}
    println!("cargo:rerun-if-changed={dir}/libxlib_go.a");
}}
"#,
        dir = archive_dir.display(),
    )
}
