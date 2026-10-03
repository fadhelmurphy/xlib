//! Sumber C++: fungsi bebas (free function) di header `include/*.hpp`.
//!
//! Tipe yang didukung: bool, int32_t/int, int64_t/long long, double,
//! std::string, std::vector<double>, std::vector<int64_t>, void.
//! Fungsi tanpa `noexcept` dianggap bisa melempar exception, dan exception
//! itu diteruskan sebagai error ke bahasa target.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use regex::Regex;

use super::{safe_ident, Func, Param, Parsed, Skipped, Ty};

pub fn headers(project: &Path) -> Result<Vec<PathBuf>> {
    let mut v = list(&project.join("include"), &["hpp", "h", "hh"])?;
    v.sort();
    if v.is_empty() {
        bail!("tidak ada header di include/ (mis. include/mathlib.hpp)");
    }
    Ok(v)
}

pub fn sources(project: &Path) -> Result<Vec<PathBuf>> {
    let mut v = list(&project.join("src"), &["cpp", "cc", "cxx"])?;
    v.sort();
    Ok(v)
}

fn list(dir: &Path, exts: &[&str]) -> Result<Vec<PathBuf>> {
    if !dir.exists() {
        return Ok(vec![]);
    }
    Ok(fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().and_then(|e| e.to_str()).is_some_and(|e| exts.contains(&e)))
        .collect())
}

fn map_type(raw: &str) -> Option<Ty> {
    let t = raw.replace("const", "").replace(['&'], "").replace("std::", "").split_whitespace().collect::<Vec<_>>().join(" ");
    Some(match t.as_str() {
        "void" => Ty::Void,
        "bool" => Ty::Bool,
        "int" | "int32_t" => Ty::I32,
        "int64_t" | "long long" | "long" => Ty::I64,
        "double" => Ty::F64,
        "string" | "string_view" => Ty::Str,
        "vector<double>" => Ty::VecF64,
        "vector<int64_t>" | "vector<long long>" => Ty::VecI64,
        _ => return None,
    })
}

pub fn parse(project: &Path) -> Result<Parsed> {
    // Hapus komentar, lalu cari deklarasi fungsi di level namespace.
    let block_comment = Regex::new(r"(?s)/\*.*?\*/").unwrap();
    let line_comment = Regex::new(r"//[^\n]*").unwrap();
    let ns_re = Regex::new(r"^\s*namespace\s+([\w:]+)\s*\{").unwrap();
    let fn_re = Regex::new(
        r"^\s*(?:inline\s+|static\s+|extern\s+)*([\w:<>,\s]+?[\s&*])\s*(\w+)\s*\(([^()]*)\)\s*(noexcept)?\s*;",
    )
    .unwrap();

    let mut funcs = Vec::new();
    let mut skipped = Vec::new();

    for header in headers(project)? {
        let text = fs::read_to_string(&header).with_context(|| header.display().to_string())?;
        let text = line_comment.replace_all(&block_comment.replace_all(&text, ""), "").to_string();

        // Lacak namespace per baris lewat kedalaman kurung kurawal.
        let mut ns_stack: Vec<(String, usize)> = Vec::new();
        let mut depth = 0usize;
        let mut pending = String::new();

        for line in text.lines() {
            if let Some(c) = ns_re.captures(line) {
                ns_stack.push((c[1].to_string(), depth + 1));
            }
            // Deklarasi bisa terpecah beberapa baris; gabungkan sampai ketemu ';' atau '{'.
            if depth == ns_stack.last().map_or(0, |n| n.1) && !line.trim_start().starts_with('#') {
                pending.push_str(line);
                pending.push(' ');
                if line.contains(';') || line.contains('{') {
                    if let Some(c) = fn_re.captures(&pending) {
                        let name = c[2].to_string();
                        let qual = ns_stack.iter().map(|n| n.0.as_str()).collect::<Vec<_>>();
                        match build_func(&c[1], &name, &c[3], c.get(4).is_some()) {
                            Ok(mut f) => {
                                f.source_name = if qual.is_empty() { name } else { format!("{}::{name}", qual.join("::")) };
                                funcs.push(f);
                            }
                            Err(reason) => skipped.push(Skipped { name, reason }),
                        }
                    }
                    pending.clear();
                }
            }
            depth += line.matches('{').count();
            depth = depth.saturating_sub(line.matches('}').count());
            while ns_stack.last().is_some_and(|n| depth < n.1) {
                ns_stack.pop();
            }
        }
    }
    Ok(Parsed { funcs, skipped })
}

fn build_func(ret: &str, name: &str, params: &str, noexcept: bool) -> std::result::Result<Func, String> {
    let ret_ty = map_type(ret).ok_or_else(|| format!("tipe balik `{}` belum didukung", ret.trim()))?;
    let mut ps = Vec::new();
    let params = params.trim();
    if !params.is_empty() && params != "void" {
        for (i, p) in params.split(',').enumerate() {
            let p = p.split('=').next().unwrap().trim(); // buang nilai default
            let (ty, pname) = match p.rfind(|c: char| c.is_whitespace() || c == '&' || c == '*') {
                Some(idx) if p[idx + 1..].chars().all(|c| c.is_alphanumeric() || c == '_') && !p[idx + 1..].is_empty() => {
                    (&p[..=idx], p[idx + 1..].to_string())
                }
                _ => (p, format!("arg{i}")),
            };
            let ty_raw = ty;
            let ty = map_type(ty).ok_or_else(|| format!("parameter `{p}` belum didukung"))?;
            if ty == Ty::Void {
                return Err("parameter void".into());
            }
            ps.push(Param { name: pname, ty, raw: ty_raw.trim().to_string() });
        }
    }
    Ok(Func {
        source_name: name.to_string(),
        name: name.to_string(),
        params: ps,
        ret: ret_ty,
        fallible: !noexcept,
    })
}

/// Wrapper `extern "C"` yang menangkap exception dan mengonversi tipe.
pub fn wrapper(headers: &[PathBuf], funcs: &[Func]) -> String {
    let mut s = String::from("// Dibuat otomatis oleh xlib. Jangan diedit.\n");
    for h in headers {
        let _ = writeln!(s, "#include \"{}\"", h.display());
    }
    s.push_str(
        r#"#include <cstdint>
#include <cstdlib>
#include <cstring>
#include <exception>
#include <string>
#include <vector>

struct XlibErr { char* msg; };

static char* xlib_dup(const std::string& v) {
    char* p = static_cast<char*>(std::malloc(v.size() + 1));
    std::memcpy(p, v.data(), v.size());
    p[v.size()] = 0;
    return p;
}

template <typename T>
static T* xlib_buf(const std::vector<T>& v, size_t* out_len) {
    *out_len = v.size();
    if (v.empty()) return nullptr;
    T* p = static_cast<T*>(std::malloc(sizeof(T) * v.size()));
    std::memcpy(p, v.data(), sizeof(T) * v.size());
    return p;
}

extern "C" {
"#,
    );

    for f in funcs {
        let mut args = Vec::new();
        let mut call = Vec::new();
        for p in &f.params {
            let n = safe_ident(&p.name);
            match p.ty {
                Ty::Str => {
                    args.push(format!("const char* {n}"));
                    call.push(format!("std::string({n})"));
                }
                Ty::VecF64 => {
                    args.extend([format!("const double* {n}_ptr"), format!("size_t {n}_len")]);
                    call.push(format!("std::vector<double>({n}_ptr, {n}_ptr + {n}_len)"));
                }
                Ty::VecI64 => {
                    args.extend([format!("const int64_t* {n}_ptr"), format!("size_t {n}_len")]);
                    call.push(format!("std::vector<int64_t>({n}_ptr, {n}_ptr + {n}_len)"));
                }
                t => {
                    args.push(format!("{} {n}", c_type(t)));
                    call.push(n);
                }
            }
        }
        if f.ret.is_vec() {
            args.push("size_t* out_len".into());
        }
        args.push("XlibErr* err".into());
        let invoke = format!("{}({})", f.source_name, call.join(", "));
        let (ret_c, body, fallback) = match f.ret {
            Ty::Void => ("void".to_string(), format!("{invoke}; return;"), "return;".to_string()),
            Ty::Str => ("char*".to_string(), format!("return xlib_dup({invoke});"), "return nullptr;".to_string()),
            Ty::VecF64 => ("double*".to_string(), format!("return xlib_buf({invoke}, out_len);"), "return nullptr;".to_string()),
            Ty::VecI64 => ("int64_t*".to_string(), format!("return xlib_buf({invoke}, out_len);"), "return nullptr;".to_string()),
            t => (c_type(t).to_string(), format!("return {invoke};"), "return {};".to_string()),
        };
        let _ = writeln!(
            s,
            "\n{ret_c} {}({}) {{\n    try {{ {body} }}\n    catch (const std::exception& e) {{ err->msg = xlib_dup(e.what()); }}\n    catch (...) {{ err->msg = xlib_dup(\"exception C++ tidak dikenal\"); }}\n    {fallback}\n}}",
            f.c_symbol(),
            args.join(", ")
        );
    }
    s.push_str("\n} // extern \"C\"\n");
    s
}

fn c_type(t: Ty) -> &'static str {
    match t {
        Ty::Bool => "bool",
        Ty::I32 => "int32_t",
        Ty::I64 => "int64_t",
        Ty::F64 => "double",
        _ => unreachable!(),
    }
}

pub fn build_rs(project: &Path, headers: &[PathBuf], sources: &[PathBuf], wrapper: &Path) -> String {
    let files: Vec<String> = sources.iter().chain(std::iter::once(&wrapper.to_path_buf())).map(|p| format!("{:?}", p.display().to_string())).collect();
    let rerun: Vec<String> = headers.iter().chain(sources).map(|p| format!("    println!(\"cargo:rerun-if-changed={}\");", p.display())).collect();
    format!(
        r#"// Dibuat otomatis oleh xlib. Compile kode C++ modul + wrapper C ABI.
fn main() {{
    cc::Build::new()
        .cpp(true)
        .std("c++17")
        .include({include:?})
        .files([{files}])
        .warnings(false)
        .compile("xlib_cpp");
{rerun}
}}
"#,
        include = project.join("include").display().to_string(),
        files = files.join(", "),
        rerun = rerun.join("\n"),
    )
}
