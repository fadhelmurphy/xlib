# xlib

Tulis modul sekali di Rust, lalu pakai dari Python, Kotlin, Swift, Go, dan (nanti) JavaScript. Begitu saja idenya.

![demo](docs/demo.gif)

## Kenapa bikin ini

Saya sering ketemu logika yang sama ditulis ulang di beberapa tempat. Contohnya validasi atau perhitungan yang ada di backend Python, lalu harus ditulis lagi di aplikasi Android dan iOS. Setiap ada perubahan, harus ingat update di tiga tempat, dan biasanya ada satu yang ketinggalan.

Sebenarnya alatnya sudah ada. [UniFFI](https://github.com/mozilla/uniffi-rs) dari Mozilla bisa membuat binding dari Rust ke banyak bahasa. Tapi setiap mau build, saya harus ingat urutan perintahnya, nama library-nya, flag-nya, dan cara merapikan hasilnya per bahasa. `xlib` pada dasarnya membungkus semua itu jadi satu perintah dengan menu yang bisa dicentang.

## Cara pakai

Install CLI-nya dulu:

```bash
git clone https://github.com/fadhelmurphy/xlib
cd xlib/cli
cargo install --path .
```

Buat modul baru:

```bash
xlib init mathlib
cd mathlib
```

Template-nya sudah berisi contoh fungsi biasa, struct, object yang punya state, dan error. Silakan ubah `src/lib.rs` sesuai kebutuhan.

Lalu build:

```bash
xlib build
```

Akan muncul menu untuk memilih bahasa target. Pilihan terakhir diingat, jadi build berikutnya tinggal tekan enter.

Untuk CI atau kalau malas pakai menu:

```bash
xlib build --target python,kotlin
xlib build --all
xlib build --force    # abaikan cache
```

Hasilnya masuk ke `dist/<bahasa>/`. Di tiap folder ada `CARA_PAKAI.txt` berisi langkah singkat untuk bahasa tersebut.

Kalau ada target yang belum bisa di-build, jalankan:

```bash
xlib doctor
```

Perintah ini mengecek toolchain mana yang sudah siap dan mana yang belum.

## Contoh pemakaian

Folder `examples/mathlib/consumers/` berisi contoh memanggil modul yang sama dari beberapa bahasa.

Python:

```python
import mathlib

print(mathlib.add(2, 3))
print(mathlib.describe([4.0, 8.0, 15.0, 16.0]))
```

Kotlin:

```kotlin
import uniffi.mathlib.*

fun main() {
    println(add(2, 3))
    println(describe(listOf(4.0, 8.0, 15.0, 16.0)))
}
```

Error dari Rust otomatis jadi exception di Python dan Kotlin, jadi tidak perlu cek kode error manual.

## Status

Masih prototipe. Yang sudah saya coba sendiri:

- **Python**: jalan, bisa di-`pip install`
- **Kotlin**: jalan di JVM (butuh JNA)
- **Rust**: jalan, karena memang crate biasa
- **Swift**: binding, header, dan modulemap sudah dibuat, tapi belum saya tes di Xcode
- **Go**: butuh [`uniffi-bindgen-go`](https://github.com/NordSecurity/uniffi-bindgen-go), belum saya tes end-to-end
- **JavaScript**: belum ada

## Hal yang perlu diketahui

Dua hal ini sempat bikin saya bingung, jadi saya catat di sini:

- Versi UniFFI dikunci ke `0.31`, karena `uniffi-bindgen-go` belum mengikuti versi terbaru. Kalau dinaikkan, binding Go kemungkinan besar rusak.
- Jangan pakai `strip = true` di profile release. Metadata UniFFI ikut terhapus, dan hasilnya error `No UniFFI metadata found` yang tidak jelas penyebabnya.

## Rencana

- [ ] Target JavaScript (napi-rs untuk Node, wasm untuk browser)
- [ ] Packaging Swift jadi XCFramework / Swift Package
- [ ] Packaging Kotlin jadi AAR untuk Android
- [ ] Perintah `xlib publish` ke PyPI, Maven, npm, dan seterusnya
- [ ] Backend Wasm Component Model sebagai alternatif UniFFI

Desainnya memang disiapkan supaya backend bisa diganti. Kalau suatu saat Wasm Component Model sudah cukup matang, kode modul tidak perlu diubah, cukup ganti `backend` di `xlib.toml`.

## Struktur repo

```
cli/                  kode CLI xlib
cli/templates/        template yang dipakai `xlib init`
examples/mathlib/     contoh modul + consumer di beberapa bahasa
```

Menambah bahasa baru cukup dengan menambah satu entri di `cli/src/targets.rs`.

## Lisensi

MIT
