package dev.morpho.domain.stream

/**
 * JavaScript's whitespace (`\s` and `String.trim`): ASCII spaces plus the Unicode space
 * separators, line separators and the byte-order mark. Text is split and trimmed by this
 * set so words fall exactly where the web client's do.
 */
fun isJsSpace(c: Char): Boolean = when (c) {
    '\t', '\n', '\u000B', '\u000C', '\r', ' ', ' ', ' ', ' ', ' ',
    ' ', ' ', '　', '﻿',
    -> true
    in ' '..' ' -> true
    else -> false
}

/** One or more [isJsSpace] characters. */
internal val jsSpaces = Regex("[\\t\\n\\u000B\\f\\r \\u00A0\\u1680\\u2000-\\u200A\\u2028\\u2029\\u202F\\u205F\\u3000\\uFEFF]+")

/** An ASCII word character on either side ends a match, as JavaScript's `\b` does. */
private const val WORD_START = "(?<![A-Za-z0-9_])"
private const val WORD_END = "(?![A-Za-z0-9_])"

/**
 * The headword and its regular forms inside running text, case-insensitively, as whole
 * words: the lemma, +s/es/ed/d/ing/'s; e-final stem + ing/ed; y-final stem + ies/ied; a
 * doubled final b d g k l m n p r t + ed/ing. A multi-word headword matches with any run of
 * spaces between its words. The web client's `wordPattern` follows the same rule; word
 * boundaries are ASCII, as JavaScript's `\b` is, on every platform.
 */
fun wordPattern(word: String): Regex? {
    val w = word.trim(::isJsSpace).lowercase()
    if (w.isEmpty()) return null
    if (w.any(::isJsSpace)) {
        val joined = w.split(jsSpaces).joinToString(jsSpaces.pattern) { Regex.escape(it) }
        return Regex(WORD_START + joined + WORD_END, RegexOption.IGNORE_CASE)
    }
    val alts = mutableListOf("${Regex.escape(w)}(?:s|es|ed|d|ing|'s)?")
    if (w.endsWith("e")) alts += "${Regex.escape(w.dropLast(1))}(?:ing|ed)"
    if (w.endsWith("y")) alts += "${Regex.escape(w.dropLast(1))}(?:ies|ied)"
    val last = w.last()
    if (last in "bdgklmnprt") alts += "${Regex.escape(w)}$last(?:ed|ing)"
    return Regex("$WORD_START(?:${alts.joinToString("|")})$WORD_END", RegexOption.IGNORE_CASE)
}
