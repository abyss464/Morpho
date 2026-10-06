package dev.morpho.ui.stream

import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import dev.morpho.domain.model.Example
import dev.morpho.domain.model.WordBundle

/**
 * The headword and its regular forms inside running text, case-insensitively, as whole
 * words: the lemma, +s/es/ed/d/ing/'s; e-final stem + ing/ed; y-final stem + ies/ied; a
 * doubled final b d g k l m n p r t + ed/ing. A multi-word headword matches with any run of
 * spaces between its words. Same rule as the web client's `wordPattern`.
 */
internal fun wordPattern(word: String): Regex? {
    val w = word.trim().lowercase()
    if (w.isEmpty()) return null
    if (w.contains(Regex("\\s"))) {
        return Regex("\\b" + w.split(Regex("\\s+")).joinToString("\\s+") { Regex.escape(it) } + "\\b", RegexOption.IGNORE_CASE)
    }
    val alts = mutableListOf("${Regex.escape(w)}(?:s|es|ed|d|ing|'s)?")
    if (w.endsWith("e")) alts += "${Regex.escape(w.dropLast(1))}(?:ing|ed)"
    if (w.endsWith("y")) alts += "${Regex.escape(w.dropLast(1))}(?:ies|ied)"
    val last = w.last()
    if (last in "bdgklmnprt") alts += "${Regex.escape(w)}$last(?:ed|ing)"
    return Regex("\\b(?:${alts.joinToString("|")})\\b", RegexOption.IGNORE_CASE)
}

private val Bold = SpanStyle(fontWeight = FontWeight.SemiBold)

/** [text] with every form of [word] set in semibold. */
internal fun markWord(text: String, word: String): AnnotatedString = buildAnnotatedString {
    append(text)
    wordPattern(word)?.findAll(text)?.forEach { addStyle(Bold, it.range.first, it.range.last + 1) }
}

/** The example with its highlighted span set in semibold; [Marked.range] is that span. */
internal data class Marked(val text: AnnotatedString, val range: IntRange)

internal fun markExample(example: Example, word: String): Marked {
    val range = example.highlightCharRange().takeUnless { it.isEmpty() }
        ?: wordPattern(word)?.find(example.sentence)?.range
        ?: IntRange.EMPTY
    val text = buildAnnotatedString {
        append(example.sentence)
        if (!range.isEmpty()) addStyle(Bold, range.first, range.last + 1)
    }
    return Marked(text, range)
}

/** The sentence of the use step split around the word to fill in. */
data class Gap(val before: String, val target: String, val after: String)

/**
 * Where the word goes in the use step: the highlighted span of its example; else the first
 * form of the word found in the example, then in its definition. A word with neither (the
 * release's integrity scan rules it out) is asked as a bare blank.
 */
internal fun gapOf(word: WordBundle): Gap {
    val example = word.cardExample
    if (example != null) {
        val range = example.highlightCharRange()
        if (!range.isEmpty()) return example.sentence.gapAround(range)
    }
    val pattern = wordPattern(word.word.word)
    listOfNotNull(example?.sentence, word.primarySense.definition).forEach { text ->
        pattern?.find(text)?.let { return text.gapAround(it.range) }
    }
    return Gap("", word.word.word, "")
}

private fun String.gapAround(range: IntRange) =
    Gap(substring(0, range.first), substring(range.first, range.last + 1), substring(range.last + 1))
