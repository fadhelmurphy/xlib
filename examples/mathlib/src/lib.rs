//! Modul contoh. Semua yang diberi `#[uniffi::export]` / `#[derive(uniffi::...)]`
//! otomatis tersedia di Python, Kotlin, Swift, Go, dst.

use std::sync::Mutex;

uniffi::setup_scaffolding!();

/// Fungsi biasa.
#[uniffi::export]
pub fn add(a: i64, b: i64) -> i64 {
    a + b
}

#[uniffi::export]
pub fn greet(name: String) -> String {
    format!("Halo, {name}! (dari Rust)")
}

/// Struct data -> jadi dataclass / data class / struct di bahasa target.
#[derive(uniffi::Record, Debug, Clone)]
pub struct Stats {
    pub count: u64,
    pub mean: f64,
    pub min: f64,
    pub max: f64,
}

/// Error -> jadi exception di Python/Kotlin/Swift, error di Go.
#[derive(uniffi::Error, thiserror::Error, Debug)]
pub enum MathError {
    #[error("data kosong")]
    EmptyInput,
}

#[uniffi::export]
pub fn describe(values: Vec<f64>) -> Result<Stats, MathError> {
    if values.is_empty() {
        return Err(MathError::EmptyInput);
    }
    let count = values.len() as u64;
    let sum: f64 = values.iter().sum();
    let min = values.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    Ok(Stats { count, mean: sum / count as f64, min, max })
}

/// Object dengan state -> jadi class di bahasa target.
#[derive(uniffi::Object)]
pub struct Counter {
    value: Mutex<i64>,
}

#[uniffi::export]
impl Counter {
    #[uniffi::constructor]
    pub fn new(start: i64) -> Self {
        Self { value: Mutex::new(start) }
    }

    pub fn increment(&self) -> i64 {
        let mut v = self.value.lock().unwrap();
        *v += 1;
        *v
    }

    pub fn get(&self) -> i64 {
        *self.value.lock().unwrap()
    }
}
