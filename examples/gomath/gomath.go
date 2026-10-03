// Modul contoh dalam Go. Setiap fungsi yang diekspor (huruf kapital) otomatis
// tersedia di Python, Kotlin, Swift, dst. setelah `xlib build`.
//
// Tipe yang didukung: bool, int, int32, int64, float64, string, []float64,
// []int64. Kalau fungsi mengembalikan error, di bahasa target error itu
// muncul sebagai exception.
package gomath

import (
	"errors"
	"fmt"
)

func Add(a, b int64) int64 {
	return a + b
}

func Greet(name string) string {
	return fmt.Sprintf("Halo, %s! (dari Go)", name)
}

func Mean(values []float64) (float64, error) {
	if len(values) == 0 {
		return 0, errors.New("data kosong")
	}
	sum := 0.0
	for _, v := range values {
		sum += v
	}
	return sum / float64(len(values)), nil
}

func Scale(values []float64, factor float64) []float64 {
	out := make([]float64, len(values))
	for i, v := range values {
		out[i] = v * factor
	}
	return out
}
