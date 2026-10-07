package dev.morpho.ui.stream

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.size
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.unit.dp
import dev.morpho.R
import dev.morpho.domain.model.WordBundle
import dev.morpho.domain.stream.Spelling
import dev.morpho.domain.stream.wordPattern
import dev.morpho.ui.designsystem.component.GlossedText
import dev.morpho.ui.designsystem.motion.floatAnimatable
import dev.morpho.ui.designsystem.motion.pressMotion
import dev.morpho.ui.designsystem.motion.runShake
import dev.morpho.ui.designsystem.theme.MorphoTheme

/**
 * The primary definition with every form of the word replaced by a short copper rule, so it
 * names nothing: the clue for spelling the word.
 */
@Composable
fun MaskedDefinition(word: WordBundle, modifier: Modifier = Modifier) {
    val definition = word.primarySense.definition
    val masked = remember(definition, word.word.word) {
        val rules = mutableListOf<IntRange>()
        val text = buildAnnotatedString {
            var last = 0
            wordPattern(word.word.word)?.findAll(definition)?.forEach { m ->
                append(definition, last, m.range.first)
                val start = length
                append(MASK)
                rules += start until length
                last = m.range.last + 1
            }
            append(definition, last, definition.length)
        }
        text to rules
    }
    GlossedText(
        text = masked.first,
        style = MorphoTheme.reading.definition,
        color = MaterialTheme.colorScheme.onSurface,
        underlines = masked.second,
        underlineColor = MorphoTheme.accents.motifActive,
        modifier = modifier.semantics { contentDescription = masked.first.text.replace(MASK, "the word") },
    )
}

/** The look of one letter cell. */
private enum class Cell { GIVEN, OPEN, NEXT, FILLED, WRONG, SOLVED }

/**
 * The word as letter cells: given letters on the panel in muted ink, open blanks dashed (the
 * one the next tile fills in copper), filled blanks as letters that go back when tapped.
 * After a failed check the wrong letters turn red and the row shakes once; once solved the
 * letters turn green. The status line sits under it.
 */
@Composable
fun SpellBoard(state: SpellState, onReturn: (Int) -> Unit, modifier: Modifier = Modifier) {
    val spacing = MorphoTheme.spacing
    val tokens = MorphoTheme.tokens
    val reducedMotion = MorphoTheme.reducedMotion
    val amplitude = with(LocalDensity.current) { tokens.sizes.shakeAmplitude.toPx() }
    val shake = remember { floatAnimatable() }
    LaunchedEffect(state.misses) {
        if (state.misses > 0 && state.wrongShown && !reducedMotion) shake.runShake(amplitude, tokens.durations.shake)
    }
    val boardLabel = stringResource(R.string.cd_spelling)
    Column(modifier = modifier, verticalArrangement = Arrangement.spacedBy(spacing.xs)) {
        FlowRow(
            modifier = Modifier
                .fillMaxWidth()
                .graphicsLayer { translationX = shake.value }
                .semantics {
                    contentDescription = boardLabel
                    liveRegion = LiveRegionMode.Polite
                },
            horizontalArrangement = Arrangement.spacedBy(LETTER_GAP),
            verticalArrangement = Arrangement.spacedBy(LETTER_GAP),
        ) {
            var blank = 0
            state.puzzle.slots.forEach { given ->
                if (given != null) {
                    if (Spelling.isLetter(given)) {
                        LetterCell(given, Cell.GIVEN)
                    } else {
                        Separator(given)
                    }
                    return@forEach
                }
                val k = blank++
                val id = state.filled[k]
                if (id == null) {
                    LetterCell("", if (k == state.nextOpen && !state.solved) Cell.NEXT else Cell.OPEN)
                } else {
                    val letter = state.letterOf(id)
                    LetterCell(
                        letter = letter,
                        look = when {
                            state.solved -> Cell.SOLVED
                            state.wrongShown && !state.right(k) -> Cell.WRONG
                            else -> Cell.FILLED
                        },
                        onClick = if (state.solved) null else ({ onReturn(k) }),
                        description = stringResource(R.string.cd_piece_remove, letter),
                    )
                }
            }
        }
        StatusText(
            text = stringResource(
                when {
                    state.solved -> R.string.stream_fill_solved
                    state.wrongShown -> R.string.stream_spell_wrong
                    else -> R.string.stream_spell_hint
                },
            ),
            tone = when {
                state.solved -> Tone.GOOD
                state.wrongShown -> Tone.BAD
                else -> Tone.NEUTRAL
            },
        )
    }
}

@Composable
private fun LetterCell(
    letter: String,
    look: Cell,
    onClick: (() -> Unit)? = null,
    description: String? = null,
) {
    val colors = MaterialTheme.colorScheme
    val accents = MorphoTheme.accents
    val radius = MorphoTheme.radii.sm
    val (fill, ink) = when (look) {
        Cell.GIVEN -> colors.surfaceContainerHigh to colors.onSurfaceVariant
        Cell.OPEN, Cell.NEXT -> colors.background to colors.onSurface
        Cell.FILLED -> colors.surfaceContainerLow to colors.onSurface
        Cell.WRONG -> accents.wrongContainer to accents.wrong
        Cell.SOLVED -> accents.correctContainer to colors.onSurface
    }
    val line = when (look) {
        Cell.GIVEN -> Color.Transparent
        Cell.OPEN -> openLine()
        Cell.FILLED -> colors.outlineVariant
        Cell.NEXT -> accents.motifActive
        Cell.WRONG -> accents.wrong
        Cell.SOLVED -> accents.correct
    }
    Box(
        modifier = Modifier
            .size(width = MorphoTheme.sizes.letterWidth, height = MorphoTheme.sizes.letterHeight)
            .then(if (onClick != null) Modifier.pressMotion() else Modifier)
            .clip(MorphoTheme.radii.shapeSm)
            .background(fill)
            .drawBehind {
                val dashed = look == Cell.OPEN
                val width = (if (look == Cell.NEXT) 2 else 1).dp.toPx()
                drawRoundRect(
                    color = line,
                    topLeft = androidx.compose.ui.geometry.Offset(width / 2, width / 2),
                    size = androidx.compose.ui.geometry.Size(size.width - width, size.height - width),
                    cornerRadius = CornerRadius(radius.toPx()),
                    style = Stroke(
                        width = width,
                        pathEffect = if (dashed) PathEffect.dashPathEffect(floatArrayOf(4.dp.toPx(), 3.dp.toPx())) else null,
                    ),
                )
            }
            .then(if (onClick != null) Modifier.clickable(onClick = onClick) else Modifier)
            .then(
                if (description != null) {
                    Modifier.clearAndSetSemantics { contentDescription = description }
                } else {
                    Modifier
                },
            ),
        contentAlignment = Alignment.Center,
    ) {
        Text(text = letter, style = MorphoTheme.reading.letter, color = ink)
    }
}

/** A space or hyphen inside the word: narrow, no box. */
@Composable
private fun Separator(character: String) {
    Box(
        modifier = Modifier.size(width = MorphoTheme.sizes.letterGap, height = MorphoTheme.sizes.letterHeight),
        contentAlignment = Alignment.Center,
    ) {
        Text(text = character, style = MorphoTheme.reading.letter, color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}

/** The letter tiles on offer; a tile in the word keeps its slot here, empty, so nothing jumps. */
@Composable
fun LetterTiles(state: SpellState, onPlace: (Int) -> Unit, modifier: Modifier = Modifier) {
    if (state.solved) return
    val tilesLabel = stringResource(R.string.cd_letters)
    FlowRow(
        modifier = modifier
            .fillMaxWidth()
            .semantics { contentDescription = tilesLabel },
        horizontalArrangement = Arrangement.spacedBy(MorphoTheme.spacing.xs),
        verticalArrangement = Arrangement.spacedBy(MorphoTheme.spacing.xs),
    ) {
        state.puzzle.tiles.forEach { tile ->
            val used = state.used(tile.id)
            val colors = MaterialTheme.colorScheme
            Box(
                modifier = Modifier
                    .size(MorphoTheme.spacing.minTouchTarget)
                    .pressMotion(!used)
                    .clip(MorphoTheme.radii.shapeSm)
                    .background(if (used) colors.surfaceContainerHigh else colors.surfaceContainerLow)
                    .border(BorderStroke(1.dp, if (used) Color.Transparent else MorphoTheme.accents.ringTrack), MorphoTheme.radii.shapeSm)
                    .clickable(enabled = !used) { onPlace(tile.id) }
                    .then(
                        if (used) {
                            Modifier.clearAndSetSemantics { }
                        } else {
                            Modifier.semantics { contentDescription = tile.letter }
                        },
                    ),
                contentAlignment = Alignment.Center,
            ) {
                Text(
                    text = tile.letter,
                    style = MorphoTheme.reading.letter,
                    color = if (used) Color.Transparent else colors.onSurface,
                )
            }
        }
    }
}

/** The ways out, always there until the word is right: the next letter, the word, a fresh start. */
@Composable
fun SpellActions(
    state: SpellState,
    onShowNextLetter: () -> Unit,
    onShowWord: () -> Unit,
    onStartOver: () -> Unit,
    modifier: Modifier = Modifier,
) {
    if (state.solved) return
    FlowRow(modifier = modifier.fillMaxWidth()) {
        LinkButton(stringResource(R.string.stream_show_next_letter), onShowNextLetter)
        LinkButton(stringResource(R.string.stream_show_word), onShowWord)
        LinkButton(stringResource(R.string.stream_start_over), onStartOver, enabled = state.filled.any { it != null })
    }
}

/** Eight figure spaces hold the masked word's width; the rule under them is drawn. */
private const val MASK = "        "

/** Space between letter cells. */
private val LETTER_GAP = 6.dp
