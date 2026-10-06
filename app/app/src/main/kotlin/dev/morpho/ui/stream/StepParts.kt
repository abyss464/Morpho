package dev.morpho.ui.stream

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import dev.morpho.R
import dev.morpho.data.stream.WordNote
import dev.morpho.domain.model.WordBundle
import dev.morpho.ui.designsystem.component.AudioChipButton
import dev.morpho.ui.designsystem.component.ContentImage
import dev.morpho.ui.designsystem.component.GlossedText
import dev.morpho.ui.designsystem.component.MarkKind
import dev.morpho.ui.designsystem.component.MorphoMark
import dev.morpho.ui.designsystem.theme.MorphoSectionLabel
import dev.morpho.ui.designsystem.theme.MorphoTheme
import dev.morpho.ui.designsystem.theme.PHONETIC_ALPHA

/** The motif mark of a step label: square for a new word, diamonds for learning and review. */
enum class StageMark { NEW, LEARN, REVIEW }

/** The step's kind as the learner reads it, with its motif, in the corner of the card. */
@Composable
fun StageLabel(mark: StageMark, text: String, modifier: Modifier = Modifier) {
    val accents = MorphoTheme.accents
    Row(
        modifier = modifier,
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(MorphoTheme.spacing.xs),
    ) {
        when (mark) {
            StageMark.NEW -> MorphoMark(MarkKind.SQUARE, accents.motifBase, size = 8.dp)
            StageMark.LEARN -> MorphoMark(MarkKind.DIAMOND, accents.motifActive, size = 8.dp)
            StageMark.REVIEW -> MorphoMark(MarkKind.DIAMOND, accents.motifMastered, size = 8.dp)
        }
        Text(
            text = text.uppercase(),
            style = MorphoSectionLabel,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

/**
 * The word's picture (its example's image), cropped to the given shape. A word without
 * one shows the word itself on the sunken ground.
 */
@Composable
fun WordPicture(word: WordBundle, modifier: Modifier = Modifier) {
    val file = word.word.imageFile
    val shaped = modifier.clip(MorphoTheme.radii.shapeMd)
    if (file.isBlank()) {
        Box(
            shaped.background(MaterialTheme.colorScheme.surfaceContainerHigh),
            contentAlignment = Alignment.Center,
        ) {
            Text(
                text = word.word.word,
                style = MorphoTheme.reading.wordOption,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.semantics { contentDescription = "No picture" },
            )
        }
        return
    }
    ContentImage(
        file = file,
        contentDescription = stringResource(R.string.cd_picture, word.word.word),
        modifier = shaped,
    )
}

/** A full-width 4:3 picture, as on the word card and the use step. */
@Composable
fun WidePicture(word: WordBundle, modifier: Modifier = Modifier) {
    WordPicture(word, modifier.fillMaxWidth().aspectRatio(4f / 3f))
}

/** The headword, its phonetic and part of speech, and the play button beside them. */
@Composable
fun WordTitle(
    word: WordBundle,
    playing: Boolean,
    onPlay: () -> Unit,
    playLabel: String,
    modifier: Modifier = Modifier,
) {
    Row(
        modifier = modifier.fillMaxWidth(),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.SpaceBetween,
    ) {
        Column(Modifier.weight(1f)) {
            Text(
                text = word.word.word,
                style = MorphoTheme.reading.wordHeadline,
                color = MaterialTheme.colorScheme.onSurface,
            )
            val phonetic = word.word.phonetic
            Row(horizontalArrangement = Arrangement.spacedBy(MorphoTheme.spacing.xs)) {
                if (!phonetic.isNullOrBlank()) {
                    Text(
                        text = phonetic,
                        style = MorphoTheme.reading.phonetic,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = PHONETIC_ALPHA),
                    )
                }
                Text(
                    text = posLabel(word.primarySense.pos),
                    style = MorphoTheme.reading.phonetic.copy(fontStyle = FontStyle.Italic),
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
        AudioChipButton(onClick = onPlay, playing = playing, contentDescription = playLabel)
    }
}

/** The primary definition with the headword in semibold, on the copper tint. */
@Composable
fun DefinitionText(word: WordBundle, modifier: Modifier = Modifier, compact: Boolean = false) {
    val definition = word.primarySense.definition
    val text = remember(definition, word.word.word) { markWord(definition, word.word.word) }
    GlossedText(
        text = text,
        style = if (compact) MorphoTheme.reading.definitionCompact else MorphoTheme.reading.definition,
        color = MaterialTheme.colorScheme.onSurface,
        modifier = if (compact) {
            modifier
        } else {
            modifier
                .clip(MorphoTheme.radii.shapeXs)
                .background(MorphoTheme.accents.highlight)
                .padding(horizontal = MorphoTheme.spacing.xxs)
        },
    )
}

/** The example sentence, its headword in semibold over a copper rule. */
@Composable
fun ExampleText(word: WordBundle, modifier: Modifier = Modifier) {
    val example = word.cardExample ?: return
    val marked = remember(example, word.word.word) { markExample(example, word.word.word) }
    GlossedText(
        text = marked.text,
        style = MorphoTheme.reading.sentence,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        underlines = listOf(marked.range),
        underlineColor = MorphoTheme.accents.motifActive,
        modifier = modifier,
    )
}

/** The learner's own explanation of the word, when there is one. */
@Composable
fun NoteBlock(note: WordNote?, modifier: Modifier = Modifier) {
    if (note == null) return
    Column(
        modifier = modifier
            .fillMaxWidth()
            .clip(MorphoTheme.radii.shapeSm)
            .background(MaterialTheme.colorScheme.surfaceContainerLow)
            .padding(MorphoTheme.spacing.sm),
        verticalArrangement = Arrangement.spacedBy(MorphoTheme.spacing.xxs),
    ) {
        Text(
            text = stringResource(R.string.stream_note_title).uppercase(),
            style = MorphoSectionLabel,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Text(
            text = note.text,
            style = MorphoTheme.reading.definitionCompact,
            color = MaterialTheme.colorScheme.onSurface,
        )
    }
}

/**
 * The full word card: picture, the step label, word with phonetic, part of speech and a
 * speaker that reads word, definition and example, then the definition, the example and the
 * learner's note. The stream's `know` step and a looked-up word both show it.
 */
@Composable
fun WordCard(
    word: WordBundle,
    mark: StageMark,
    label: String,
    note: WordNote?,
    playing: Boolean,
    onPlay: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(modifier = modifier, verticalArrangement = Arrangement.spacedBy(MorphoTheme.spacing.sm)) {
        WidePicture(word)
        StageLabel(mark = mark, text = label)
        WordTitle(
            word = word,
            playing = playing,
            onPlay = onPlay,
            playLabel = stringResource(R.string.cd_play_card),
        )
        DefinitionText(word)
        ExampleText(word)
        NoteBlock(note)
    }
}

/** A one-line status under a task: neutral, good or bad. */
enum class Tone { NEUTRAL, GOOD, BAD }

@Composable
fun StatusText(text: String, tone: Tone, modifier: Modifier = Modifier) {
    Text(
        text = text,
        style = MaterialTheme.typography.bodySmall,
        color = when (tone) {
            Tone.NEUTRAL -> MaterialTheme.colorScheme.onSurfaceVariant
            Tone.GOOD -> MorphoTheme.accents.correct
            Tone.BAD -> MorphoTheme.accents.wrong
        },
        modifier = modifier,
    )
}

/** The full-width pill at the foot of every step; dims while the step is unsolved. */
@Composable
fun PrimaryAction(
    text: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    outlined: Boolean = false,
) {
    val primary = MaterialTheme.colorScheme.primary
    Button(
        onClick = onClick,
        enabled = enabled,
        shape = MorphoTheme.radii.shapeFull,
        colors = if (outlined) {
            ButtonDefaults.outlinedButtonColors(contentColor = primary)
        } else {
            ButtonDefaults.buttonColors(
                disabledContainerColor = primary.copy(alpha = DISABLED_ALPHA),
                disabledContentColor = MaterialTheme.colorScheme.onPrimary.copy(alpha = 0.8f),
            )
        },
        border = if (outlined) {
            androidx.compose.foundation.BorderStroke(1.dp, MorphoTheme.accents.motifBase)
        } else {
            null
        },
        modifier = modifier
            .fillMaxWidth()
            .heightIn(min = MorphoTheme.sizes.primaryButton),
    ) {
        Text(text = text, style = MaterialTheme.typography.titleSmall, textAlign = TextAlign.Center)
    }
}

/** Fills the space it is given with the sunken ground and a centred mark: a covered picture. */
@Composable
fun CoveredPicture(modifier: Modifier = Modifier, content: @Composable () -> Unit) {
    Box(
        modifier
            .clip(MorphoTheme.radii.shapeMd)
            .background(MaterialTheme.colorScheme.surfaceContainerHigh),
        contentAlignment = Alignment.Center,
    ) {
        Column(
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(MorphoTheme.spacing.xs),
        ) {
            Text(
                text = "?",
                style = MorphoTheme.reading.prompt,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            content()
        }
    }
}

private const val DISABLED_ALPHA = 0.4f

private val POS = mapOf(
    "noun" to "n.",
    "verb" to "v.",
    "adj" to "adj.",
    "adv" to "adv.",
    "prep" to "prep.",
    "conj" to "conj.",
    "interj" to "interj.",
    "phrase" to "phr.",
)

/** "noun" -> "n.", as the web client labels parts of speech. */
fun posLabel(pos: String): String = POS[pos] ?: "$pos."
