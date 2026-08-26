package dev.morpho.ui.designsystem.component

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ElevatedCard
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.withStyle
import dev.morpho.ui.designsystem.theme.MorphoTheme

/**
 * Mode-1 stimulus: an English example sentence in serif with the target word wearing
 * a primary-container pill. Tapping anywhere plays the sentence audio.
 *
 * [highlight] is a **character** range — convert the release DB's UTF-8 byte offsets
 * with `Example.highlightCharRange()` before calling.
 */
@Composable
fun SentenceCard(
    sentence: String,
    highlight: IntRange,
    onPlayAudio: () -> Unit,
    modifier: Modifier = Modifier,
    playing: Boolean = false,
    compact: Boolean = false,
) {
    val text = rememberHighlightedSentence(sentence, highlight)
    ElevatedCard(
        modifier = modifier
            .fillMaxWidth()
            .clickable(onClick = onPlayAudio)
            .semantics { contentDescription = "Example sentence. $sentence. Tap to listen." },
        shape = MorphoTheme.radii.shapeMd,
        colors = CardDefaults.elevatedCardColors(
            containerColor = MaterialTheme.colorScheme.surfaceContainerLow,
        ),
        elevation = CardDefaults.elevatedCardElevation(
            defaultElevation = MorphoTheme.elevations.raised,
        ),
    ) {
        Column(Modifier.padding(MorphoTheme.spacing.lg)) {
            // The card plays audio on tap; a tap that lands on an anchored word opens
            // its gloss instead. Neither reading is an answer, so nothing is ambiguous.
            GlossedText(
                text = text,
                style = if (compact) {
                    MorphoTheme.reading.sentenceCompact
                } else {
                    MorphoTheme.reading.sentence
                },
                color = MaterialTheme.colorScheme.onSurface,
                onPlainTap = onPlayAudio,
            )
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(top = MorphoTheme.spacing.xs),
                horizontalArrangement = Arrangement.End,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                AudioChipButton(
                    onClick = onPlayAudio,
                    playing = playing,
                    small = true,
                    contentDescription = "Play the example sentence",
                )
            }
        }
    }
}

@Composable
private fun rememberHighlightedSentence(sentence: String, highlight: IntRange): AnnotatedString {
    val accents = MorphoTheme.accents
    return buildAnnotatedString {
        val start = highlight.first.coerceIn(0, sentence.length)
        val end = (highlight.last + 1).coerceIn(start, sentence.length)
        if (start >= end) {
            append(sentence)
            return@buildAnnotatedString
        }
        append(sentence.substring(0, start))
        withStyle(
            SpanStyle(
                background = accents.highlight,
                color = accents.onHighlight,
                fontWeight = FontWeight.SemiBold,
                textDecoration = TextDecoration.None,
            ),
        ) {
            append(sentence.substring(start, end))
        }
        append(sentence.substring(end))
    }
}

@ThemePreviews
@Composable
private fun SentenceCardPreview() {
    PreviewBox {
        Column(verticalArrangement = Arrangement.spacedBy(MorphoTheme.spacing.md)) {
            SentenceCard(
                sentence = "A benevolent stranger paid for the whole table without saying a word.",
                highlight = 2..11,
                onPlayAudio = {},
            )
            SentenceCard(
                sentence = "The committee will scrutinize every claim before it signs anything.",
                highlight = 22..30,
                onPlayAudio = {},
                playing = true,
                compact = true,
            )
        }
    }
}
