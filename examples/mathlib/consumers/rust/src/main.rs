use mathlib::*;

fn main() {
    println!("{}", add(2, 3));
    println!("{}", greet("Rust".into()));
    println!("{:?}", describe(vec![4.0, 8.0, 15.0, 16.0]).unwrap());
    let c = Counter::new(10); c.increment();
    println!("counter: {}", c.increment());
}
