package dev.morpho.ui.stream

import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import dev.morpho.domain.model.Example
import dev.morpho.domain.model.WordBundle
import dev.morpho.domain.stream.wordPattern

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
