package dev.morpho.domain.stream

import dev.morpho.domain.model.WordBundle

/**
 * The "explain it" puzzle (docs/contracts/stream.md §4): the learner completes a word's
 * definition by tapping pieces into its blanks in order. The part that names the word, the
 * word wherever it appears, prepositions and punctuation are given in place; everything
 * else is a blank. Decoy pieces from other words' definitions are mixed in. A faithful port
 * of the web client's `explain.ts`: for the same word and pool the puzzle comes out the same.
 */
enum class Difficulty(val maxWords: Int, val maxBlanks: Int, val decoys: Int) {
    /** Whole clauses; decoys from the words being learned alongside. */
    EASY(7, 6, 2),

    /** Finer pieces; decoys from every word already met. */
    HARD(4, 8, 3),
}

data class Piece(val id: Int, val text: String)

/** One stretch of the definition: text given in place, or the blank with this index. */
sealed interface Segment {
    data class Given(val text: String) : Segment

    data class Blank(val index: Int) : Segment
}

data class Puzzle(
    /** The definition in order: given text and numbered blanks. */
    val template: List<Segment>,
    /** Every piece on offer, shuffled: one per blank plus a few decoys. */
    val pieces: List<Piece>,
    /** For each blank, the id of the piece that fills it. */
    val answer: List<Int>,
)

object Pieces {
    private const val MIN_WORDS = 2

    /** The word must be named within this many words of a sense's start for it to have an opening. */
    private const val OPENING_REACH = 12

    /** An "If" clause closes within this many words after the word, or the opening ends at the word. */
    private const val CLAUSE_REACH = 8

    // A new piece starts before these words, so pieces follow the definition's own clauses.
    private val breakBefore = setOf(
        "that", "who", "which", "where", "when", "whose", "because", "but",
        "if", "without", "such", "than", "while", "until", "unless",
    )
    private val joiners = setOf("or", "and")

    // The definition's grammar rather than its meaning: given in place, never picked.
    private val prepositions = setOf(
        "of", "in", "on", "at", "for", "from", "by", "with", "about", "into", "onto", "to", "as",
        "over", "under", "through", "between", "among", "across", "against", "during", "within",
        "without", "behind", "below", "above", "around", "along", "towards", "toward", "upon",
        "beneath", "beyond",
    )

    // First words of two-word prepositions ("because of", "such as"), given with them.
    private val leads = setOf("because", "instead", "according", "due", "apart", "such", "out")

    // An opening runs on through a copula right after the word ("A system is", "a case is also"),
    // or, in an "If"/"When" clause, to that clause's comma ("If you create something,").
    private val copula = setOf("is", "are", "means", "mean", "was", "were")
    private val openers = setOf("if", "when")

    private val endsClause = Regex("[,;:]$")
    private val trailing = Regex("[,;:.!?]+$")
    private val onlyPunct = Regex("^[,;:.!?]+$")
    private val onlyClausePunct = Regex("^[,;:]+$")
    private val stops = Regex("[;.!?]")

    fun norm(text: String): String = text.trim().lowercase()

    private fun bare(word: String): String = word.lowercase().filter { it in 'a'..'z' || it == '\'' }

    /** What is given in place: openings and the word itself, or prepositions. */
    private enum class Given { NAME, GLUE }

    private fun givenWords(text: String, words: List<String>, word: String, glue: Boolean): Array<Given?> {
        val marks = arrayOfNulls<Given>(words.size)
        val starts = IntArray(words.size)
        var at = 0
        words.forEachIndexed { k, w ->
            at = text.indexOf(w, at)
            starts[k] = at
            at += w.length
        }
        fun wordAt(offset: Int): Int {
            var k = 0
            while (k + 1 < starts.size && starts[k + 1] <= offset) k += 1
            return k
        }

        // The word itself, every time it appears.
        val named = BooleanArray(words.size)
        wordPattern(word)?.findAll(text)?.forEach { m ->
            val last = wordAt(m.range.last)
            for (k in wordAt(m.range.first)..last) named[k] = true
        }

        // Each sense (senses are separated by ";") may open by naming the word.
        var from = 0
        for (k in words.indices) {
            if (!words[k].endsWith(";") && k < words.size - 1) continue
            val to = k + 1
            val hit = (from until to).firstOrNull { named[it] } ?: -1
            if (hit >= 0 && hit - from < OPENING_REACH) {
                var end = hit
                while (end < to && named[end]) end += 1
                val closed = endsClause.containsMatchIn(words[end - 1])
                var c = end
                if (bare(words.getOrElse(c) { "" }) == "also") c += 1
                if (!closed && bare(words.getOrElse(c) { "" }) in copula && c + 1 < to) {
                    end = c + 1
                    if (bare(words.getOrElse(end) { "" }) == "also") end += 1
                } else if (!closed && bare(words[from]) in openers) {
                    val comma = words.indices.firstOrNull { i ->
                        i >= end && i < minOf(to - 1, end + CLAUSE_REACH) && endsClause.containsMatchIn(words[i])
                    } ?: -1
                    if (comma >= 0) end = comma + 1
                }
                if (end < to) for (i in from until end) marks[i] = Given.NAME
            }
            from = to
        }

        words.forEachIndexed { k, w ->
            if (named[k]) {
                marks[k] = Given.NAME
            } else if (glue && marks[k] == null) {
                if (bare(w) in prepositions) {
                    marks[k] = Given.GLUE
                } else if (bare(w) in leads && bare(words.getOrElse(k + 1) { "" }) in prepositions) {
                    marks[k] = Given.GLUE
                }
            }
        }
        return marks
    }

    /** Where to cut a run of words in two: before "or"/"and" nearest the middle, else the middle. */
    private fun cutAt(words: List<String>): Int {
        val mid = words.size / 2
        for (d in words.indices) {
            for (k in intArrayOf(mid - d, mid + d)) {
                if (k >= MIN_WORDS && words.size - k >= MIN_WORDS && bare(words[k]) in joiners) return k
            }
        }
        return mid
    }

    /** Splits a run of words between given text into clause-sized pieces. */
    private fun pieces(run: List<String>, max: Int): List<MutableList<String>> {
        val raw = mutableListOf<List<String>>()
        var cur = mutableListOf<String>()
        for (word in run) {
            // "or because" stays together: a piece never ends on a joiner.
            if (cur.size >= MIN_WORDS && bare(word) in breakBefore && bare(cur.last()) !in joiners) {
                raw += cur
                cur = mutableListOf()
            }
            cur += word
            if (endsClause.containsMatchIn(word) && cur.size >= MIN_WORDS) {
                raw += cur
                cur = mutableListOf()
            }
        }
        if (cur.isNotEmpty()) raw += cur

        val split = mutableListOf<List<String>>()
        for (piece in raw) {
            var rest = piece
            while (rest.size > max) {
                val at = cutAt(rest)
                split += rest.subList(0, at)
                rest = rest.subList(at, rest.size)
            }
            split += rest
        }

        // One-word scraps fold into a neighbour within the run.
        val merged = mutableListOf<MutableList<String>>()
        for (piece in split) {
            val prev = merged.lastOrNull()
            if (prev != null && (piece.size < MIN_WORDS || prev.size < MIN_WORDS)) prev += piece
            else merged += piece.toMutableList()
        }
        return merged
    }

    private sealed interface Part {
        class Text(val given: Given, val words: MutableList<String>) : Part

        class Hole(val words: List<String>) : Part
    }

    /**
     * Cuts a definition into given text and blanks, at most [Difficulty.maxBlanks] of them.
     * Prepositions are blanks only when the meaning is nothing else ("Regarding means about.").
     */
    private fun cut(def: String, word: String, difficulty: Difficulty, glue: Boolean = true): List<Part> {
        val text = def.trim(::isJsSpace)
        val words = text.split(jsSpaces)
        val marks = givenWords(text, words, word, glue)
        val parts = mutableListOf<Part>()
        fun give(w: String, kind: Given) {
            val last = parts.lastOrNull()
            if (last is Part.Text && (last.given == kind || onlyPunct.matches(w))) last.words += w
            else parts += Part.Text(kind, mutableListOf(w))
        }
        var run = mutableListOf<String>()
        fun flush() {
            for (p in pieces(run, difficulty.maxWords)) {
                // Closing punctuation is given, so a piece's own ending never shows where it goes.
                val last = p.last()
                val punct = trailing.find(last)?.value
                if (punct != null && last.length > punct.length) {
                    parts += Part.Hole(p.dropLast(1) + last.dropLast(punct.length))
                    give(punct, Given.GLUE)
                } else {
                    parts += Part.Hole(p)
                }
            }
            run = mutableListOf()
        }
        words.forEachIndexed { k, w ->
            val mark = marks[k]
            if (mark != null) {
                flush()
                give(w, mark)
            } else {
                run += w
            }
        }
        flush()

        // Too many blanks: join the shortest pair that only prepositions or nothing separate.
        while (parts.count { it is Part.Hole } > difficulty.maxBlanks) {
            var best = -1
            var bestSize = Int.MAX_VALUE
            for (i in parts.indices) {
                val a = parts[i] as? Part.Hole ?: continue
                var j = i + 1
                val between = mutableListOf<String>()
                while (j < parts.size) {
                    val g = parts[j] as? Part.Text ?: break
                    if (g.given != Given.GLUE || g.words.any { stops.containsMatchIn(it) }) break
                    between += g.words
                    j += 1
                }
                val b = parts.getOrNull(j) as? Part.Hole ?: continue
                if (j > i + 1 && between.isEmpty()) continue
                val size = a.words.size + between.size + b.words.size
                if (size < bestSize) {
                    best = i
                    bestSize = size
                }
            }
            if (best < 0) break
            var j = best + 1
            val joined = (parts[best] as Part.Hole).words.toMutableList()
            while (true) {
                val g = parts[j] as? Part.Text ?: break
                for (w in g.words) {
                    if (onlyClausePunct.matches(w)) joined[joined.size - 1] = joined.last() + w
                    else joined += w
                }
                j += 1
            }
            joined += (parts[j] as Part.Hole).words
            for (r in best..j) parts.removeAt(best)
            parts.add(best, Part.Hole(joined))
        }

        val only = parts.filterIsInstance<Part.Hole>()
        if (only.isEmpty() && glue) return cut(def, word, difficulty, glue = false)
        // A meaning of four words or more is never a single blank: there is always an order to rebuild.
        if (only.size == 1) {
            val p = only[0]
            if (p.words.size >= 2 * MIN_WORDS) {
                val at = cutAt(p.words)
                val i = parts.indexOf(p)
                parts.removeAt(i)
                parts.addAll(i, listOf(Part.Hole(p.words.subList(0, at)), Part.Hole(p.words.subList(at, p.words.size))))
            }
        }
        return parts
    }

    /** The pieces that fill a definition's blanks, in order. */
    private fun blankTexts(parts: List<Part>): List<String> =
        parts.filterIsInstance<Part.Hole>().map { it.words.joinToString(" ") }

    /**
     * Builds the puzzle for [word]. Decoys are pieces of the [pool] words' definitions: easy
     * puzzles draw from the words being learned alongside, hard ones from every word met.
     * Shuffled with the word id, moved on by [attempt] for a step done again.
     */
    fun puzzle(word: WordBundle, pool: List<WordBundle>, difficulty: Difficulty, attempt: Int = 0): Puzzle {
        val parts = cut(word.primarySense.definition, word.word.word, difficulty)
        val answerTexts = blankTexts(parts)
        val rand = Seeded(attemptSeed(word.word.wordId, attempt))
        val own = answerTexts.map { it.lowercase() }.toSet()
        val candidates = pool.filter { it.word.wordId != word.word.wordId }.flatMap { other ->
            val sense = other.senses.firstOrNull { it.isPrimary } ?: other.senses.firstOrNull()
            if (sense == null) {
                emptyList()
            } else {
                blankTexts(cut(sense.definition, other.word.word, difficulty)).filter { it.lowercase() !in own }
            }
        }
        val count = difficulty.decoys + if (answerTexts.size >= 5) 1 else 0
        val decoys = rand.shuffle(candidates.distinct()).take(count)
        var blank = 0
        val template = parts.map { p ->
            when (p) {
                is Part.Hole -> Segment.Blank(blank++)
                is Part.Text -> Segment.Given(p.words.joinToString(" "))
            }
        }
        val all = (answerTexts + decoys).mapIndexed { id, text -> Piece(id, text) }
        return Puzzle(template = template, pieces = rand.shuffle(all), answer = answerTexts.indices.toList())
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
