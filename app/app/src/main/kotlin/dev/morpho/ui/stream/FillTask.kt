package dev.morpho.ui.stream

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import dev.morpho.R
import dev.morpho.domain.model.WordBundle
import dev.morpho.ui.designsystem.component.GlossedText
import dev.morpho.ui.designsystem.motion.floatAnimatable
import dev.morpho.ui.designsystem.motion.pressScale
import dev.morpho.ui.designsystem.motion.runShake
import dev.morpho.ui.designsystem.theme.MorphoTheme

/** The example with its gap: a copper rule while open, the word in semibold on it once filled. */
@Composable
fun GapSentence(state: FillState, modifier: Modifier = Modifier) {
    val gap = state.gap
    val middle = if (state.solved) gap.target else BLANK
    val text = remember(gap, state.solved) {
        buildAnnotatedString {
            append(gap.before)
            if (state.solved) {
                withStyle(SpanStyle(fontWeight = FontWeight.SemiBold)) { append(middle) }
            } else {
                append(middle)
            }
            append(gap.after)
        }
    }
    GlossedText(
        text = text,
        style = MorphoTheme.reading.gapSentence,
        color = MaterialTheme.colorScheme.onSurface,
        underlines = listOf(gap.before.length until gap.before.length + middle.length),
        underlineColor = MorphoTheme.accents.motifActive,
        modifier = modifier,
    )
}

/**
 * The four words, two by two. A wrong pick turns red, shakes once and stays disabled; the
 * right pick turns green.
 */
@Composable
fun WordOptions(state: FillState, onPick: (Long) -> Unit, modifier: Modifier = Modifier) {
    val spacing = MorphoTheme.spacing
    Column(modifier = modifier, verticalArrangement = Arrangement.spacedBy(spacing.sm)) {
        Column(
            modifier = Modifier.selectableGroup(),
            verticalArrangement = Arrangement.spacedBy(spacing.sm),
        ) {
            state.options.chunked(2).forEach { row ->
                Row(horizontalArrangement = Arrangement.spacedBy(spacing.sm)) {
                    row.forEach { option ->
                        WordOption(
                            word = option,
                            right = state.solved && option.word.wordId == state.answerId,
                            wrong = option.word.wordId in state.wrong,
                            enabled = !state.solved && option.word.wordId !in state.wrong,
                            onClick = { onPick(option.word.wordId) },
                            modifier = Modifier.weight(1f),
                        )
                    }
                    if (row.size == 1) Spacer(Modifier.weight(1f))
                }
            }
        }
        StatusText(
            text = stringResource(
                when {
                    state.solved -> R.string.stream_fill_solved
                    state.wrong.isNotEmpty() -> R.string.stream_fill_wrong
                    else -> R.string.stream_fill_hint
                },
            ),
            tone = when {
                state.solved -> Tone.GOOD
                state.wrong.isNotEmpty() -> Tone.BAD
                else -> Tone.NEUTRAL
            },
        )
    }
}

@Composable
private fun WordOption(
    word: WordBundle,
    right: Boolean,
    wrong: Boolean,
    enabled: Boolean,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val tokens = MorphoTheme.tokens
    val accents = MorphoTheme.accents
    val colors = MaterialTheme.colorScheme
    val reducedMotion = MorphoTheme.reducedMotion
    val interaction = remember { MutableInteractionSource() }
    val pressed by interaction.collectIsPressedAsState()
    val amplitude = with(LocalDensity.current) { tokens.sizes.shakeAmplitude.toPx() }
    val shake = remember { floatAnimatable() }
    LaunchedEffect(wrong) {
        if (wrong && !reducedMotion) shake.runShake(amplitude, tokens.durations.shake)
    }
    val (background, border, content) = when {
        right -> Triple(accents.correctContainer, accents.correct, accents.correct)
        wrong -> Triple(accents.wrongContainer, accents.wrong, accents.wrong)
        else -> Triple(colors.surfaceContainerLow, colors.outlineVariant, colors.onSurface)
    }
    Box(
        modifier = modifier
            .heightIn(min = tokens.sizes.wordOption)
            .graphicsLayer { translationX = shake.value }
            .pressScale(pressed && enabled, reducedMotion)
            .clip(tokens.radii.shapeSm)
            .background(background)
            .border(BorderStroke(1.dp, border), tokens.radii.shapeSm)
            .selectable(
                selected = right,
                enabled = enabled,
                role = Role.RadioButton,
                interactionSource = interaction,
                indication = null,
                onClick = onClick,
            )
            .padding(horizontal = tokens.spacing.md, vertical = tokens.spacing.xs),
        contentAlignment = Alignment.Center,
    ) {
        Text(text = word.word.word, style = MorphoTheme.reading.wordOption, color = content)
    }
}

/**
 * Each wrongly picked word with its own primary definition. Shown under the sentence, so
 * the options below never move while the learner is picking.
 */
@Composable
fun WrongMeanings(state: FillState, modifier: Modifier = Modifier) {
    Column(modifier = modifier, verticalArrangement = Arrangement.spacedBy(MorphoTheme.spacing.xs)) {
        state.wrong.forEach { id ->
            state.options.firstOrNull { it.word.wordId == id }?.let { WhyNot(it) }
        }
    }
}

@Composable
private fun WhyNot(option: WordBundle) {
    val text = remember(option) {
        buildAnnotatedString {
            withStyle(SpanStyle(fontWeight = FontWeight.SemiBold)) { append(option.word.word) }
            append(": ")
            append(option.primarySense.definition)
        }
    }
    Text(
        text = text,
        style = MorphoTheme.reading.definitionCompact,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        modifier = Modifier
            .fillMaxWidth()
            .clip(MorphoTheme.radii.shapeSm)
            .background(MorphoTheme.accents.wrongContainer.copy(alpha = 0.5f))
            .padding(MorphoTheme.spacing.sm),
    )
}

/** Eight figure spaces hold the blank's width; the rule under them is drawn. */
private const val BLANK = "        "
