#pragma once
// Modul contoh dalam C++. Setiap fungsi bebas di header ini otomatis
// tersedia di Python, Kotlin, Swift, Go, dst. setelah `xlib build`.
//
// Tipe yang didukung: bool, int32_t, int64_t, double, std::string,
// std::vector<double>, std::vector<int64_t>.
// Fungsi tanpa `noexcept` boleh melempar exception; di bahasa target
// exception itu muncul sebagai error biasa.

#include <cstdint>
#include <string>
#include <vector>

namespace {{lib_name}} {

int64_t add(int64_t a, int64_t b) noexcept;

std::string greet(const std::string& name) noexcept;

// Melempar std::invalid_argument kalau datanya kosong.
double mean(const std::vector<double>& values);

std::vector<double> scale(const std::vector<double>& values, double factor) noexcept;

}  // namespace {{lib_name}}
