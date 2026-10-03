# xlib (prototipe)

Tulis modul sekali di Rust, pakai di Python, Kotlin, Swift, Go (dan JS menyusul).
Backend saat ini: UniFFI. Desainnya disiapkan supaya backend bisa diganti ke Wasm Component nanti.

## Install CLI

```bash
cd cli && cargo install --path .
```

## Pakai

```bash
xlib init mathlib          # scaffold modul baru (contoh: fungsi, record, object, error)
cd mathlib
xlib build                 # menu interaktif, pilihan terakhir diingat
xlib build --target python,go
xlib build --all           # semua target yang siap
xlib build --force         # abaikan cache
xlib doctor                # cek toolchain tiap target
```

Hasil build ada di `dist/<target>/`, masing-masing berisi `CARA_PAKAI.txt`.

## Status target

| Target | Status |
|---|---|
| Python | ✔ teruji (install via pip, panggil fungsi/record/object/error) |
| Kotlin | ✔ binding dibuat, butuh JNA saat runtime |
| Swift  | ✔ binding + header + modulemap dibuat; XCFramework butuh macOS |
| Go     | butuh `uniffi-bindgen-go` (lihat `xlib doctor`) |
| JS     | belum (rencana: napi-rs / wasm) |

## Catatan

- UniFFI dikunci ke `=0.31` supaya cocok dengan `uniffi-bindgen-go`.
- Jangan pakai `strip = true` di profile release, karena metadata UniFFI ikut terhapus.
- Menambah bahasa baru = tambah satu entri di `cli/src/targets.rs`.
