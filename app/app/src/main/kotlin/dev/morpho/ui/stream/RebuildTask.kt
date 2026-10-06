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
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
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
import androidx.compose.ui.unit.dp
import dev.morpho.R
import dev.morpho.domain.stream.Segment
import dev.morpho.ui.designsystem.motion.floatAnimatable
import dev.morpho.ui.designsystem.motion.pressMotion
import dev.morpho.ui.designsystem.motion.runShake
import dev.morpho.ui.designsystem.theme.MorphoTheme

/** How a piece is painted. */
private enum class PieceLook { PLAIN, WRONG, SOLVED, USED }

/** Punctuation that closes a piece; given text starting with it hugs the blank before. */
private val leadingPunct = Regex("^[,;:.!?]")

/**
 * The definition as a flowing sentence in a dashed tray: given text in place, open blanks as
 * short rules (copper for the one the next piece fills), filled blanks as chips that go back
 * to the bank when tapped. After a failed check the wrong chips turn red and the tray shakes
 * once; once solved the chips and the border turn green. The status line sits under it.
 */
@Composable
fun RebuildTray(
    state: RebuildState,
    onReturn: (Int) -> Unit,
    modifier: Modifier = Modifier,
) {
    val spacing = MorphoTheme.spacing
    val tokens = MorphoTheme.tokens
    val reducedMotion = MorphoTheme.reducedMotion
    val dashed = MaterialTheme.colorScheme.outlineVariant
    val solvedLine = MorphoTheme.accents.correct
    val amplitude = with(LocalDensity.current) { tokens.sizes.shakeAmplitude.toPx() }
    val shake = remember { floatAnimatable() }
    LaunchedEffect(state.misses) {
        if (state.misses > 0 && state.wrongShown && !reducedMotion) shake.runShake(amplitude, tokens.durations.shake)
    }
    val trayLabel = stringResource(R.string.cd_tray)
    val blankLabel = stringResource(R.string.cd_blank)
    val nextLabel = stringResource(R.string.cd_next_blank)

    Column(modifier = modifier, verticalArrangement = Arrangement.spacedBy(spacing.xs)) {
        FlowRow(
            modifier = Modifier
                .fillMaxWidth()
                .graphicsLayer { translationX = shake.value }
                .clip(MorphoTheme.radii.shapeMd)
                .background(MaterialTheme.colorScheme.surfaceContainerHigh)
                .drawBehind {
                    drawRoundRect(
                        color = if (state.solved) solvedLine else dashed,
                        cornerRadius = CornerRadius(tokens.radii.md.toPx()),
                        style = Stroke(
                            width = 1.dp.toPx(),
                            pathEffect = if (state.solved) null else PathEffect.dashPathEffect(floatArrayOf(6.dp.toPx(), 4.dp.toPx())),
                        ),
                    )
                }
                .padding(horizontal = spacing.sm, vertical = spacing.xs)
                .semantics {
                    contentDescription = trayLabel
                    liveRegion = LiveRegionMode.Polite
                },
            verticalArrangement = Arrangement.spacedBy(spacing.xxs),
            itemVerticalAlignment = Alignment.CenterVertically,
        ) {
            state.puzzle.template.forEach { segment ->
                when (segment) {
                    is Segment.Given -> GivenText(segment.text)
                    is Segment.Blank -> {
                        val k = segment.index
                        val id = state.filled[k]
                        if (id == null) {
                            val next = k == state.nextOpen && !state.solved
                            OpenBlank(next = next, description = if (next) nextLabel else blankLabel)
                        } else {
                            val text = state.textOf(id)
                            PieceChip(
                                text = text,
                                look = when {
                                    state.solved -> PieceLook.SOLVED
                                    state.wrongShown && !state.right(k) -> PieceLook.WRONG
                                    else -> PieceLook.PLAIN
                                },
                                enabled = !state.solved,
                                onClick = { onReturn(k) },
                                description = stringResource(R.string.cd_piece_remove, text),
                                modifier = Modifier.padding(horizontal = TRAY_GAP),
                            )
                        }
                    }
                }
            }
        }
        StatusText(
            text = stringResource(
                when {
                    state.solved -> R.string.stream_rebuild_solved
                    state.wrongShown -> R.string.stream_rebuild_wrong
                    else -> R.string.stream_rebuild_hint
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

/**
 * Given text, word by word so the sentence wraps between words. Punctuation that opens it
 * sits tight against the blank before.
 */
@Composable
private fun GivenText(text: String) {
    text.split(' ').filter { it.isNotEmpty() }.forEachIndexed { i, word ->
        val hug = i == 0 && leadingPunct.containsMatchIn(word)
        Box(
            modifier = Modifier
                .heightIn(min = MorphoTheme.spacing.minTouchTarget)
                .padding(start = if (hug) 0.dp else TRAY_GAP, end = TRAY_GAP),
            contentAlignment = Alignment.CenterStart,
        ) {
            Text(text = word, style = MorphoTheme.reading.piece, color = MaterialTheme.colorScheme.onSurface)
        }
    }
}

/** An empty blank: a short rule, copper when it is the one the next piece fills. */
@Composable
private fun OpenBlank(next: Boolean, description: String) {
    val color = if (next) MorphoTheme.accents.motifActive else MaterialTheme.colorScheme.outlineVariant
    Box(
        modifier = Modifier
            .padding(horizontal = TRAY_GAP)
            .size(width = MorphoTheme.sizes.blankWidth, height = MorphoTheme.spacing.minTouchTarget)
            .drawBehind {
                val y = size.height * BLANK_RULE_AT
                drawLine(color, Offset(0f, y), Offset(size.width, y), strokeWidth = 2.dp.toPx())
            }
            .semantics { contentDescription = description },
    )
}

/** The pieces on offer; a piece in the tray keeps its slot here, empty, so nothing jumps. */
@Composable
fun PieceBank(state: RebuildState, onPlace: (Int) -> Unit, modifier: Modifier = Modifier) {
    if (state.solved) return
    val spacing = MorphoTheme.spacing
    val bankLabel = stringResource(R.string.cd_bank)
    FlowRow(
        modifier = modifier
            .fillMaxWidth()
            .semantics { contentDescription = bankLabel },
        horizontalArrangement = Arrangement.spacedBy(spacing.xs),
        verticalArrangement = Arrangement.spacedBy(spacing.xs),
    ) {
        state.puzzle.pieces.forEach { piece ->
            val used = state.used(piece.id)
            PieceChip(
                text = piece.text,
                look = if (used) PieceLook.USED else PieceLook.PLAIN,
                enabled = !used,
                onClick = { onPlace(piece.id) },
                description = piece.text,
            )
        }
    }
}

/** The ways out, always there until the tray is right: the next piece, the answer, a fresh start. */
@Composable
fun RebuildActions(
    state: RebuildState,
    onShowNextPiece: () -> Unit,
    onShowAnswer: () -> Unit,
    onStartOver: () -> Unit,
    modifier: Modifier = Modifier,
) {
    if (state.solved) return
    FlowRow(modifier = modifier.fillMaxWidth()) {
        LinkButton(stringResource(R.string.stream_show_next_piece), onShowNextPiece)
        LinkButton(stringResource(R.string.stream_show_answer), onShowAnswer)
        LinkButton(stringResource(R.string.stream_start_over), onStartOver, enabled = state.filled.any { it != null })
    }
}

@Composable
internal fun LinkButton(text: String, onClick: () -> Unit, enabled: Boolean = true) {
    TextButton(onClick = onClick, enabled = enabled, modifier = Modifier.pressMotion(enabled)) {
        Text(text = text, style = MaterialTheme.typography.bodyMedium)
    }
}

@Composable
private fun PieceChip(
    text: String,
    look: PieceLook,
    enabled: Boolean,
    onClick: () -> Unit,
    description: String,
    modifier: Modifier = Modifier,
) {
    val accents = MorphoTheme.accents
    val colors = MaterialTheme.colorScheme
    val (background, border, content) = when (look) {
        PieceLook.PLAIN -> Triple(colors.surfaceContainerLow, colors.outlineVariant, colors.onSurface)
        PieceLook.WRONG -> Triple(accents.wrongContainer, accents.wrong, accents.wrong)
        PieceLook.SOLVED -> Triple(accents.correctContainer, accents.correct, colors.onSurface)
        PieceLook.USED -> Triple(colors.surfaceContainerHigh, Color.Transparent, Color.Transparent)
    }
    Box(
        modifier = modifier
            .heightIn(min = MorphoTheme.spacing.minTouchTarget)
            .pressMotion(enabled)
            .clip(MorphoTheme.radii.shapeSm)
            .background(background)
            .border(BorderStroke(1.dp, border), MorphoTheme.radii.shapeSm)
            .clickable(enabled = enabled, onClick = onClick)
            .padding(horizontal = MorphoTheme.spacing.sm, vertical = MorphoTheme.spacing.xs)
            .then(
                if (look == PieceLook.USED) {
                    Modifier.clearAndSetSemantics { }
                } else {
                    Modifier.semantics { contentDescription = description }
                },
            ),
        contentAlignment = Alignment.CenterStart,
    ) {
        Text(text = text, style = MorphoTheme.reading.piece, color = content)
    }
}

/** Space either side of a word, chip or blank in the tray, as a sentence's word spacing. */
private val TRAY_GAP = 2.dp

/** The blank's rule sits at three quarters of its height, under the line of text. */
private const val BLANK_RULE_AT = 0.75f
