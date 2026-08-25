package dev.morpho.domain.util

/**
 * SplitMix64 — a tiny, fully specified PRNG.
 *
 * Used instead of [kotlin.random.Random] so that shuffles are reproducible by
 * construction and independent of any platform/stdlib version: the same seed
 * always produces the same permutation on every device and in every test run.
 */
class SplitMix64(seed: Long) {
    private var state: Long = seed

    fun nextLong(): Long {
        state += -0x61c8864680b583ebL // 0x9E3779B97F4A7C15
        var z = state
        z = (z xor (z ushr 30)) * -0x40a7b892e31b1a47L // 0xBF58476D1CE4E5B9
        z = (z xor (z ushr 27)) * -0x6b2fb644ecceee15L // 0x94D049BB133111EB
        return z xor (z ushr 31)
    }

    /** Uniform in `[0, bound)`. */
    fun nextInt(bound: Int): Int {
        require(bound > 0) { "bound must be positive" }
        val r = nextLong() ushr 1 // strip sign
        return (r % bound).toInt()
    }
}

/** Fisher-Yates shuffle driven by [SplitMix64]; pure and reproducible. */
fun <T> List<T>.deterministicShuffled(seed: Long): List<T> {
    if (size < 2) return toList()
    val rng = SplitMix64(seed)
    val out = toMutableList()
    for (i in out.lastIndex downTo 1) {
        val j = rng.nextInt(i + 1)
        val tmp = out[i]
        out[i] = out[j]
        out[j] = tmp
    }
    return out
}

/**
 * FNV-1a 64-bit over a list of longs — a stable way to fold a question's identity
 * (word, mode, round, unit) into a single shuffle seed.
 */
fun seedOf(vararg parts: Long): Long {
    var hash = -0x340d631b7bdddcdbL // 0xCBF29CE484222325
    for (part in parts) {
        var value = part
        repeat(8) {
            hash = hash xor (value and 0xFF)
            hash *= 0x100000001B3L
            value = value ushr 8
        }
    }
    return hash
}
