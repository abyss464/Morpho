package dev.morpho.domain.content

import java.text.Normalizer

/** What a masked headword reads as on screen: four underscores. */
const val HEADWORD_MASK = "____"

/**
 * Blanks out a definition's own headword so it can serve as a quiz option or prompt.
 *
 * Full-sentence learner definitions name the word they define ("If you abandon something,
 * you leave it…"). Wherever the learner still has to pick or recall the word, that
 * sentence would hand over the answer, so every whole-word, case-insensitive occurrence of
 * [lemma] or one of its regular inflections in [text] becomes [HEADWORD_MASK]. Everything
 * else — punctuation, spacing, the rest of the sentence — is kept as it was.
 *
 * Forms and word boundaries mirror the engine's self-reference check
 * (`core/crates/reconcile/src/score.rs`: `inflections` and `contains_whole_word`), so a
 * definition the engine reads as naming its headword is exactly one this masks:
 *
 *  * Forms: the lemma, +s, +es, +ed, +d, +ing; consonant+y → ies / ied; final e →
 *    stem+ing / stem+ed; a consonant after a single vowel after a consonant doubles before
 *    ing / ed, except w, x and y. Irregular forms are not generated.
 *  * Boundaries: a word character is an ASCII letter or digit, or any non-ASCII character.
 *    "resourceful" therefore does not contain "resource", while "self-abandon" and
 *    "abandon's" do contain "abandon".
 *
 * Where forms overlap at one position, the longest one that ends on a boundary is masked.
 */
fun maskHeadword(lemma: String, text: String): String {
    val forms = headwordForms(lemma)
    if (forms.isEmpty() || text.isEmpty()) return text
    val byLength = forms.distinct().sortedByDescending { it.length }

    var out: StringBuilder? = null
    var copiedUpTo = 0
    var i = 0
    while (i < text.length) {
        val match = if (i == 0 || !isWordChar(text[i - 1])) {
            byLength.firstOrNull { form -> matchesAt(text, i, form) }
        } else {
            null
        }
        if (match == null) {
            i++
            continue
        }
        val builder = out ?: StringBuilder(text.length).also { out = it }
        builder.append(text, copiedUpTo, i).append(HEADWORD_MASK)
        i += match.length
        copiedUpTo = i
    }
    val builder = out ?: return text
    return builder.append(text, copiedUpTo, text.length).toString()
}

/**
 * The lemma plus the inflections regular English suffixation produces, case-folded.
 * Port of the engine's `inflections`; see [maskHeadword].
 */
private fun headwordForms(lemma: String): List<String> {
    val folded = foldLemma(lemma)
    if (folded.isEmpty()) return emptyList()
    val forms = mutableListOf(
        folded,
        folded + "s",
        folded + "es",
        folded + "ed",
        folded + "d",
        folded + "ing",
    )
    // Code points, not UTF-16 units: the engine walks Unicode scalar values.
    val chars = folded.codePoints().toArray()
    val last = chars[chars.size - 1]
    // "carry" → "carries", "carried".
    if (last == 'y'.code && chars.size > 1 && !isVowel(chars[chars.size - 2])) {
        val stem = String(chars, 0, chars.size - 1)
        forms += stem + "ies"
        forms += stem + "ied"
    }
    // "charge" → "charging", "charged" (the bare +d is already above).
    if (last == 'e'.code) {
        val stem = String(chars, 0, chars.size - 1)
        forms += stem + "ing"
        forms += stem + "ed"
    }
    // "plan" → "planning", "planned": a final consonant after a single vowel after a
    // consonant doubles. 'w', 'x' and 'y' never do.
    if (chars.size >= 3 &&
        !isVowel(last) &&
        last != 'w'.code && last != 'x'.code && last != 'y'.code &&
        isVowel(chars[chars.size - 2]) &&
        !isVowel(chars[chars.size - 3])
    ) {
        val doubled = folded + String(Character.toChars(last))
        forms += doubled + "ing"
        forms += doubled + "ed"
    }
    return forms
}

/** Whether [form] sits at [start] in [text], case-insensitively, followed by a boundary. */
private fun matchesAt(text: String, start: Int, form: String): Boolean {
    val end = start + form.length
    if (end > text.length) return false
    if (!text.regionMatches(start, form, 0, form.length, ignoreCase = true)) return false
    return end == text.length || !isWordChar(text[end])
}

/**
 * The engine's `is_word_byte`: ASCII alphanumerics, plus every byte of a multi-byte UTF-8
 * sequence — so any non-ASCII character, surrogate halves included, is a word character.
 */
private fun isWordChar(c: Char): Boolean =
    c in 'a'..'z' || c in 'A'..'Z' || c in '0'..'9' || c.code >= 0x80

private fun isVowel(codePoint: Int): Boolean = when (codePoint) {
    'a'.code, 'e'.code, 'i'.code, 'o'.code, 'u'.code -> true
    else -> false
}

/**
 * The engine's `fold_lemma`: NFC, trim, collapse internal whitespace runs to one ASCII
 * space, lowercase. "Whitespace" is the Unicode `White_Space` property.
 */
private fun foldLemma(input: String): String {
    val normalized = Normalizer.normalize(input, Normalizer.Form.NFC)
    val out = StringBuilder(normalized.length)
    var pendingSpace = false
    for (c in normalized) {
        if (isUnicodeWhiteSpace(c)) {
            pendingSpace = out.isNotEmpty()
        } else {
            if (pendingSpace) {
                out.append(' ')
                pendingSpace = false
            }
            out.append(c)
        }
    }
    return out.toString().lowercase()
}

/** Unicode `White_Space`, which is what Rust's `char::is_whitespace` tests. All BMP. */
private fun isUnicodeWhiteSpace(c: Char): Boolean = when (c.code) {
    in 0x0009..0x000D, 0x0020, 0x0085, 0x00A0, 0x1680,
    in 0x2000..0x200A, 0x2028, 0x2029, 0x202F, 0x205F, 0x3000,
    -> true
    else -> false
}
