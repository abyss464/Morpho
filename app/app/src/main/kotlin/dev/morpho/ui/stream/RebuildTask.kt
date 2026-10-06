package dev.morpho.ui.stream

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
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
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import dev.morpho.R
import dev.morpho.ui.designsystem.motion.floatAnimatable
import dev.morpho.ui.designsystem.motion.pressScale
import dev.morpho.ui.designsystem.motion.runShake
import dev.morpho.ui.designsystem.theme.MorphoTheme

/** How a piece is painted. */
private enum class PieceLook { PLAIN, WRONG, SOLVED, USED }

/**
 * The tray the learner builds the meaning in, with the status line and the two helpers
 * under it. Tapping a placed piece sends it back to the bank; after a failed check the
 * misplaced pieces turn red and the tray shakes once.
 */
@Composable
fun RebuildTray(
    state: RebuildState,
    onReturn: (Int) -> Unit,
    onStartOver: () -> Unit,
    onShowNextPiece: () -> Unit,
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

    Column(modifier = modifier, verticalArrangement = Arrangement.spacedBy(spacing.xs)) {
        FlowRow(
            modifier = Modifier
                .fillMaxWidth()
                .heightIn(min = MorphoTheme.sizes.trayMinHeight)
                .graphicsLayer { translationX = shake.value }
                .clip(MorphoTheme.radii.shapeMd)
                .background(MaterialTheme.colorScheme.surfaceContainerHigh)
                .drawBehind {
                    val stroke = 1.dp.toPx()
                    drawRoundRect(
                        color = if (state.solved) solvedLine else dashed,
                        cornerRadius = androidx.compose.ui.geometry.CornerRadius(tokens.radii.md.toPx()),
                        style = Stroke(
                            width = stroke,
                            pathEffect = if (state.solved) null else PathEffect.dashPathEffect(floatArrayOf(6.dp.toPx(), 4.dp.toPx())),
                        ),
                    )
                }
                .padding(spacing.xs)
                .semantics {
                    contentDescription = trayLabel
                    liveRegion = LiveRegionMode.Polite
                },
            horizontalArrangement = Arrangement.spacedBy(spacing.xs),
            verticalArrangement = Arrangement.spacedBy(spacing.xs),
        ) {
            if (state.placed.isEmpty()) {
                Text(
                    text = stringResource(R.string.stream_tray_empty),
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.padding(spacing.xs),
                )
            }
            state.placed.forEachIndexed { k, id ->
                val text = state.textOf(id)
                val returnLabel = stringResource(R.string.cd_piece_remove, text)
                PieceChip(
                    text = text,
                    look = when {
                        state.solved -> PieceLook.SOLVED
                        state.wrongShown && !state.right(k) -> PieceLook.WRONG
                        else -> PieceLook.PLAIN
                    },
                    enabled = !state.solved,
                    onClick = { onReturn(id) },
                    description = returnLabel,
                )
            }
        }

        Row(
            modifier = Modifier.fillMaxWidth(),
            verticalAlignment = Alignment.CenterVertically,
        ) {
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
                modifier = Modifier.weight(1f),
            )
            if (!state.solved && state.placed.isNotEmpty()) {
                TextButton(onClick = onStartOver) { Text(stringResource(R.string.stream_start_over)) }
            }
        }
        if (!state.solved && state.misses > 0) {
            TextButton(onClick = onShowNextPiece) { Text(stringResource(R.string.stream_show_next_piece)) }
        }
    }
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
            val used = piece.id in state.placed
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

@Composable
private fun PieceChip(
    text: String,
    look: PieceLook,
    enabled: Boolean,
    onClick: () -> Unit,
    description: String,
) {
    val accents = MorphoTheme.accents
    val colors = MaterialTheme.colorScheme
    val interaction = remember { MutableInteractionSource() }
    val pressed by interaction.collectIsPressedAsState()
    val (background, border, content) = when (look) {
        PieceLook.PLAIN -> Triple(colors.surfaceContainerLow, colors.outlineVariant, colors.onSurface)
        PieceLook.WRONG -> Triple(accents.wrongContainer, accents.wrong, accents.wrong)
        PieceLook.SOLVED -> Triple(accents.correctContainer, accents.correct, colors.onSurface)
        PieceLook.USED -> Triple(colors.surfaceContainerHigh, Color.Transparent, Color.Transparent)
    }
    Box(
        modifier = Modifier
            .heightIn(min = MorphoTheme.spacing.minTouchTarget)
            .pressScale(pressed && enabled, MorphoTheme.reducedMotion)
            .clip(MorphoTheme.radii.shapeSm)
            .background(background)
            .border(BorderStroke(1.dp, border), MorphoTheme.radii.shapeSm)
            .clickable(
                enabled = enabled,
                interactionSource = interaction,
                indication = null,
                onClick = onClick,
            )
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
