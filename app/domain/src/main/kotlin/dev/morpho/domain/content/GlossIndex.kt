package dev.morpho.domain.content

/**
 * One `gloss_anchors` row: a lemma that shows up inside shipped English definitions
 * but is not itself a learnable word, paired with its Chinese gloss.
 */
data class GlossAnchor(
    val wordId: Long,
    val word: String,
    val zhGloss: String,
)

/** A resolved occurrence of an anchor inside a piece of rendered text. */
data class GlossMatch(
    /** Half-open char range `[start, endExclusive)` into the scanned string. */
    val start: Int,
    val endExclusive: Int,
    val lemma: String,
    val gloss: String,
) {
    operator fun contains(charIndex: Int): Boolean = charIndex in start until endExclusive
}

/**
 * Case-insensitive, whole-word lookup from `gloss_anchors` into rendered text.
 *
 * Matching is deliberately literal — the release ships the lemma exactly as it appears
 * in the definitions it was mined from (`characterised`, `pertaining`, `condones`), so
 * no stemming is wanted or needed. Two rules make it safe to run over every definition
 * on screen:
 *
 *  * **Whole word only.** `rites` must not light up inside `favourites`. Tokens are cut
 *    at letter boundaries, allowing an internal apostrophe or hyphen so `mother-in-law`
 *    stays one token rather than three.
 *  * **Case-insensitive.** The same lemma is glossed whether it opens a sentence or not.
 *
 * The whole table is a few hundred rows, so the index is a plain hash map built once at
 * startup and scanned per string; a definition is short enough that this never shows up
 * in a frame budget.
 */
class GlossIndex private constructor(private val byLemma: Map<String, GlossAnchor>) {

    val size: Int get() = byLemma.size

    val isEmpty: Boolean get() = byLemma.isEmpty()

    /** The gloss for a single token, or null when the release does not anchor it. */
    fun gloss(token: String): String? = byLemma[token.lowercase()]?.zhGloss

    /**
     * Every anchored token in [text], in reading order and never overlapping.
     * Returns an empty list — allocating nothing beyond it — for the common case of a
     * definition with no anchors at all.
     */
    fun scan(text: String): List<GlossMatch> {
        if (byLemma.isEmpty() || text.isEmpty()) return emptyList()
        var matches: MutableList<GlossMatch>? = null
        var i = 0
        while (i < text.length) {
            if (!text[i].isTokenStart()) {
                i++
                continue
            }
            var end = i + 1
            while (end < text.length && text[end].isTokenPart(text, end)) end++
            // A trailing joiner ("well-" at a line end) is not part of the token.
            while (end > i && !text[end - 1].isTokenStart()) end--

            val anchor = byLemma[text.substring(i, end).lowercase()]
            if (anchor != null) {
                (matches ?: ArrayList<GlossMatch>(2).also { matches = it })
                    .add(GlossMatch(i, end, anchor.word, anchor.zhGloss))
            }
            i = end.coerceAtLeast(i + 1)
        }
        return matches ?: emptyList()
    }

    companion object {
        val EMPTY = GlossIndex(emptyMap())

        fun of(anchors: Collection<GlossAnchor>): GlossIndex {
            if (anchors.isEmpty()) return EMPTY
            // A duplicate lemma differing only by case would be a release defect; keep
            // the lowest word_id so the choice is at least deterministic.
            val map = HashMap<String, GlossAnchor>(anchors.size * 2)
            anchors.sortedBy { it.wordId }.forEach { map.putIfAbsent(it.word.lowercase(), it) }
            return GlossIndex(map)
        }
    }
}

private fun Char.isTokenStart(): Boolean = isLetter()

/** Letters, plus an apostrophe or hyphen that has a letter on both sides. */
private fun Char.isTokenPart(text: String, at: Int): Boolean = when {
    isLetter() -> true
    this == '\'' || this == '’' || this == '-' ->
        at + 1 < text.length && text[at + 1].isLetter()
    else -> false
}
