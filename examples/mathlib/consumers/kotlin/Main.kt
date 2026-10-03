import uniffi.mathlib.*

fun main() {
    println(add(2, 3))
    println(greet("Kotlin"))
    println(describe(listOf(4.0, 8.0, 15.0, 16.0)))
    val c = Counter(10); c.increment()
    println("counter: ${c.increment()}")
}
