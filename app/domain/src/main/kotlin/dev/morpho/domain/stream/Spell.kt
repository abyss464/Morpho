package dev.morpho.domain.stream

/** One letter tile on offer. */
data class Tile(val id: Int, val letter: String)

/**
 * The `spell` step, and the spelling that ends a rebuild review (docs/contracts/stream.md §2):
 * the word's letters as blanks, with some given, and letter tiles to fill them.
 */
data class SpellPuzzle(
    /** One entry per character: the character when given (a hint letter, a space, a hyphen), else null. */
    val slots: List<String?>,
    /** The letters the blanks take, in order. */
    val answer: List<String>,
    /** Tiles on offer, shuffled: the missing letters plus two decoys. */
    val tiles: List<Tile>,
)

object Spelling {
    // Decoy tiles come from common letters the word does not use.
    private const val DECOY_LETTERS = "etaoinshrdlucmfwypbgvk"
    private const val DECOYS = 2

    /** A word longer than this many letters gives its last letter too. */
    private const val LONG_WORD = 5

    /** Whether [character] (one code point) is a letter, as JavaScript's `\p{L}`. */
    fun isLetter(character: String): Boolean = Character.isLetter(character.codePointAt(0))

    /**
     * Builds the spelling of [word]: spaces and hyphens are given, so is the first letter,
     * and the last one too for words over five letters. Shuffled with the seed [id] + 13
     * (moved on by [attempt] for a step done again), decoys first, then the tiles. A faithful
     * port of the web client's `buildSpell`.
     */
    fun puzzle(word: String, id: Long, attempt: Int = 0): SpellPuzzle {
        val lower = word.lowercase()
        val chars = lower.codePoints().toArray().map { String(Character.toChars(it)) }
        val letters = chars.indices.filter { isLetter(chars[it]) }
        val hints = mutableSetOf<Int>()
        letters.firstOrNull()?.let { hints += it }
        if (letters.size > LONG_WORD) hints += letters.last()
        val slots = chars.mapIndexed { i, c -> if (!isLetter(c) || i in hints) c else null }
        val answer = chars.filterIndexed { i, _ -> slots[i] == null }
        val rand = Seeded(attemptSeed(id + 13, attempt))
        val decoys = rand.shuffle(DECOY_LETTERS.map { it.toString() }.filter { it !in chars }).take(DECOYS)
        val tiles = rand.shuffle(answer + decoys).mapIndexed { k, t -> Tile(k, t) }
        return SpellPuzzle(slots = slots, answer = answer, tiles = tiles)
    }
}
