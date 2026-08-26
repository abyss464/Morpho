package dev.morpho.ui.designsystem.component

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.animation.scaleIn
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Check
import androidx.compose.material.icons.rounded.Close
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import dev.morpho.ui.designsystem.motion.correctSpring
import dev.morpho.ui.designsystem.motion.floatAnimatable
import dev.morpho.ui.designsystem.motion.runShake
import dev.morpho.ui.designsystem.theme.MorphoTheme

/** One text-only option: a definition (mode 3) or a word (definition-to-word review). */
data class TextOption(
    val wordId: Long,
    val text: String,
    val pos: String? = null,
    val serif: Boolean = true,
)

/**
 * Mode-3 stimulus: four stacked definition cards, no image, no sentence.
 * Also serves the definition-to-word review question with `serif = false` options.
 */
@Composable
fun QuizTextOptions(
    options: List<TextOption>,
    onSelect: (Int) -> Unit,
    modifier: Modifier = Modifier,
    selectedIndex: Int? = null,
    correctIndex: Int? = null,
    revealed: Boolean = false,
    enabled: Boolean = true,
) {
    Column(
        modifier = modifier.fillMaxWidth(),
        verticalArrangement = Arrangement.spacedBy(MorphoTheme.spacing.sm),
    ) {
        options.forEachIndexed { index, option ->
            TextOptionCard(
                option = option,
                state = optionState(index, selectedIndex, correctIndex, revealed),
                enabled = enabled,
                onClick = { onSelect(index) },
            )
        }
    }
}

@Composable
private fun TextOptionCard(
    option: TextOption,
    state: QuizOptionState,
    enabled: Boolean,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val tokens = MorphoTheme.tokens
    val accents = MorphoTheme.accents
    val reducedMotion = MorphoTheme.reducedMotion
    val interactionSource = remember { MutableInteractionSource() }
    val pressed by interactionSource.collectIsPressedAsState()

    val amplitude = with(LocalDensity.current) { tokens.sizes.shakeAmplitude.toPx() }
    val shake = remember { floatAnimatable() }
    LaunchedEffect(state) {
        if (state == QuizOptionState.WRONG && !reducedMotion) {
            shake.runShake(amplitude, tokens.durations.shake)
        }
    }

    val ringProgress by animateFloatAsState(
        targetValue = if (state == QuizOptionState.CORRECT || state == QuizOptionState.REVEALED) 1f else 0f,
        animationSpec = correctSpring(),
        label = "ring",
    )
    val alpha by animateFloatAsState(
        targetValue = when (state) {
            QuizOptionState.DIMMED -> DIMMED_OPTION_ALPHA
            QuizOptionState.WRONG -> 0.8f
            else -> 1f
        },
        animationSpec = tween(tokens.durations.correct, easing = tokens.easings.standard),
        label = "alpha",
    )
    val scale by animateFloatAsState(
        targetValue = if (pressed && !reducedMotion) 0.97f else 1f,
        animationSpec = tween(tokens.durations.press, easing = tokens.easings.standard),
        label = "press",
    )

    val ringColor = when (state) {
        QuizOptionState.CORRECT, QuizOptionState.REVEALED -> MaterialTheme.colorScheme.primary
        QuizOptionState.WRONG -> accents.wrong
        else -> Color.Transparent
    }

    Row(
        modifier = modifier
            .fillMaxWidth()
            .heightIn(min = tokens.spacing.minTouchTarget)
            .graphicsLayer {
                translationX = shake.value
                scaleX = scale
                scaleY = scale
                this.alpha = if (reducedMotion && pressed) 0.75f else alpha
            }
            .clip(tokens.radii.shapeSm)
            .background(MaterialTheme.colorScheme.surfaceContainer)
            .border(
                width = tokens.sizes.optionRingWidth * ringProgress,
                color = ringColor.copy(alpha = if (state == QuizOptionState.WRONG) 1f else ringProgress),
                shape = tokens.radii.shapeSm,
            )
            .clickable(
                enabled = enabled,
                interactionSource = interactionSource,
                indication = null,
                onClick = onClick,
            )
            .padding(tokens.spacing.md)
            .semantics { contentDescription = option.text },
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(tokens.spacing.sm),
    ) {
        if (option.pos != null) {
            PosChip(option.pos)
        }
        // Long-press, not tap: this card is an answer button, and a tap that landed on
        // a glossed word must still mean "I choose this option".
        GlossedText(
            text = option.text,
            style = if (option.serif) {
                MorphoTheme.reading.definitionOption
            } else {
                MorphoTheme.reading.wordOption
            },
            color = MaterialTheme.colorScheme.onSurface,
            modifier = Modifier.weight(1f),
            trigger = GlossTrigger.LongPress,
            enabled = enabled,
            onPlainTap = onClick,
            pressInteractionSource = interactionSource,
        )
        AnimatedVisibility(
            visible = state == QuizOptionState.CORRECT || state == QuizOptionState.REVEALED,
            enter = scaleIn(correctSpring()),
        ) {
            SmallBadge(correct = true)
        }
        AnimatedVisibility(
            visible = state == QuizOptionState.WRONG,
            enter = scaleIn(correctSpring()),
        ) {
            SmallBadge(correct = false)
        }
    }
}

@Composable
private fun SmallBadge(correct: Boolean) {
    val accents = MorphoTheme.accents
    Box(
        Modifier
            .size(MorphoTheme.sizes.checkBadge)
            .background(
                color = if (correct) MaterialTheme.colorScheme.primary else accents.wrong,
                shape = CircleShape,
            ),
        contentAlignment = Alignment.Center,
    ) {
        Icon(
            imageVector = if (correct) Icons.Rounded.Check else Icons.Rounded.Close,
            contentDescription = if (correct) "Correct" else "Wrong",
            tint = MaterialTheme.colorScheme.onPrimary,
            modifier = Modifier.size(18.dp),
        )
    }
}

/** Part-of-speech pill used by both option cards and the detail sheet. */
@Composable
fun PosChip(pos: String, modifier: Modifier = Modifier) {
    Text(
        text = pos,
        style = MaterialTheme.typography.labelSmall,
        color = MaterialTheme.colorScheme.onSecondaryContainer,
        modifier = modifier
            .clip(MorphoTheme.radii.shapeXs)
            .background(MaterialTheme.colorScheme.secondaryContainer)
            .padding(
                horizontal = MorphoTheme.spacing.xs,
                vertical = MorphoTheme.spacing.xxs,
            ),
    )
}

private val previewDefinitions = listOf(
    TextOption(1, "kind and generous towards other people", "adj"),
    TextOption(2, "unwilling to spend money or share things", "adj"),
    TextOption(3, "showing careful attention to small details", "adj"),
    TextOption(4, "eager to argue or start a fight", "adj"),
)

@ThemePreviews
@Composable
private fun QuizTextOptionsPreview() {
    PreviewBox {
        QuizTextOptions(options = previewDefinitions, onSelect = {})
    }
}

@ThemePreviews
@Composable
private fun QuizTextOptionsRevealedPreview() {
    PreviewBox {
        QuizTextOptions(
            options = previewDefinitions,
            onSelect = {},
            selectedIndex = 2,
            correctIndex = 0,
            revealed = true,
        )
    }
}

@ThemePreviews
@Composable
private fun QuizWordOptionsPreview() {
    PreviewBox {
        QuizTextOptions(
            options = listOf(
                TextOption(1, "adapt", serif = false),
                TextOption(2, "adopt", serif = false),
                TextOption(3, "adept", serif = false),
                TextOption(4, "adroit", serif = false),
            ),
            onSelect = {},
            selectedIndex = 0,
            correctIndex = 0,
            revealed = true,
        )
    }
}
