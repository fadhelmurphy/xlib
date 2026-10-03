import mathlib

print(mathlib.add(2, 3))
print(mathlib.greet("Fadhel"))
print(mathlib.describe([4.0, 8.0, 15.0, 16.0]))

c = mathlib.Counter(10)
c.increment()
print("counter:", c.increment())

try:
    mathlib.describe([])
except mathlib.MathError.EmptyInput:
    print("error dari Rust tertangkap di Python ✔")
