package dev.morpho.domain.stream

import dev.morpho.domain.model.WordBundle

/**
 * Definition pieces for the "explain it" step (docs/contracts/stream.md §4): a definition is
 * cut into clause-sized pieces that the learner taps back into order, with decoy pieces from
 * other words mixed in. Same rules as the web client's `explain.ts`.
 */
enum class Difficulty(val maxWords: Int, val decoys: Int) {
    /** Whole clauses; decoys from the words being learned alongside. */
    EASY(7, 2),

    /** Finer pieces; decoys from every word already met. */
    HARD(4, 3),
}

data class Piece(val id: Int, val text: String)

data class Puzzle(
    /** Every piece on offer, shuffled: the definition's own pieces plus decoys. */
    val pieces: List<Piece>,
    /** The answer, as normalised piece texts in order. */
    val answer: List<String>,
)

object Pieces {
    private val breakBefore = setOf(
        "that", "who", "which", "where", "when", "whose", "because", "but",
        "if", "without", "such", "than", "while", "until", "unless",
    )
    private val joiners = setOf("or", "and")
    private val softBreak = setOf("of", "in", "on", "at", "for", "from", "by", "with", "about", "into", "to", "over", "under")
    private val copula = setOf("is", "are", "means", "mean")
    private const val MIN_WORDS = 2

    fun norm(text: String): String = text.trim().lowercase()

    /** Splits [text] into 2-6 pieces that read naturally on their own. */
    fun chunk(text: String, difficulty: Difficulty = Difficulty.EASY): List<String> {
        val words = text.trim().split(Regex("\\s+")).filter { it.isNotEmpty() }
        val raw = mutableListOf<MutableList<String>>()
        var cur = mutableListOf<String>()
        for (word in words) {
            val bare = word.lowercase().replace(Regex("[^a-z']"), "")
            if (cur.size >= MIN_WORDS && bare in breakBefore) {
                raw += cur
                cur = mutableListOf()
            }
            cur += word
            if (Regex("[,;:]$").containsMatchIn(word) && cur.size >= MIN_WORDS) {
                raw += cur
                cur = mutableListOf()
            }
        }
        if (cur.isNotEmpty()) raw += cur

        // The opening clause splits right after its verb: "A system is" | "a group of ...".
        raw.firstOrNull()?.let { first ->
            if (first.size >= 4) {
                val at = first.indices.firstOrNull { it in 1..4 && first[it].lowercase() in copula }
                if (at != null && first.size - at - 1 >= 1) {
                    raw[0] = first.subList(0, at + 1).toMutableList()
                    raw.add(1, first.subList(at + 1, first.size).toMutableList())
                }
            }
        }

        // Long pieces split nearest the middle: before "or"/"and", else before a preposition.
        val split = mutableListOf<MutableList<String>>()
        for (piece in raw) {
            var rest: List<String> = piece
            while (rest.size > difficulty.maxWords) {
                val mid = rest.size / 2
                fun near(set: Set<String>): Int {
                    for (d in rest.indices) {
                        for (k in listOf(mid - d, mid + d)) {
                            if (k >= MIN_WORDS && rest.size - k >= MIN_WORDS && rest[k].lowercase() in set) return k
                        }
                    }
                    return -1
                }
                var at = near(joiners)
                if (at < 0) at = near(softBreak)
                if (at < 0) at = mid
                split += rest.subList(0, at).toMutableList()
                rest = rest.subList(at, rest.size)
            }
            split += rest.toMutableList()
        }

        // One-word scraps fold into the previous piece; then the shortest pairs merge.
        val merged = mutableListOf<MutableList<String>>()
        for (piece in split) {
            val prev = merged.lastOrNull()
            if (prev != null && merged.size > 1 && (piece.size < MIN_WORDS || prev.size < MIN_WORDS)) prev += piece
            else if (prev != null && merged.size == 1 && piece.size < MIN_WORDS) prev += piece
            else merged += piece.toMutableList()
        }
        while (merged.size > 6) {
            var best = 0
            for (k in 1 until merged.size - 1) {
                if (merged[k].size + merged[k + 1].size < merged[best].size + merged[best + 1].size) best = k
            }
            merged[best] = (merged[best] + merged[best + 1]).toMutableList()
            merged.removeAt(best + 1)
        }
        return merged.map { it.joinToString(" ") }
    }

    /** Builds the puzzle for [word]; decoys are pieces of the [pool] words' definitions. */
    fun puzzle(word: WordBundle, pool: List<WordBundle>, difficulty: Difficulty): Puzzle {
        val parts = chunk(word.primarySense.definition, difficulty)
        val rand = Seeded(word.word.wordId)
        val own = parts.map(::norm).toSet()
        val candidates = pool.filter { it.word.wordId != word.word.wordId }.flatMap { other ->
            val head = other.word.word.lowercase()
            // Opening pieces ("A tax is") and pieces naming their own word would give it away.
            chunk(other.primarySense.definition, difficulty).drop(1)
                .filter { !it.lowercase().contains(head) && norm(it) !in own }
        }
        val count = difficulty.decoys + if (parts.size >= 5) 1 else 0
        val decoys = rand.shuffle(candidates).take(count)
        val pieces = (parts + decoys).mapIndexed { i, t -> Piece(i, t) }
        return Puzzle(pieces = rand.shuffle(pieces), answer = parts.map(::norm))
    }
}

/** A small deterministic generator, so a word's puzzle looks the same every time. */
class Seeded(seed: Long) {
    private var s: Int = seed.toInt().let { if (it == 0) 1 else it }

    fun next(): Double {
        s = s xor (s shl 13)
        s = s xor (s ushr 17)
        s = s xor (s shl 5)
        return (s.toLong() and 0xFFFFFFFFL).toDouble() / 4294967296.0
    }

    fun <T> shuffle(items: List<T>): List<T> {
        val out = items.toMutableList()
        for (i in out.size - 1 downTo 1) {
            val j = (next() * (i + 1)).toInt()
            val t = out[i]
            out[i] = out[j]
            out[j] = t
        }
        return out
    }
}
