package dev.morpho.ui.designsystem.component

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
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
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import dev.morpho.ui.designsystem.motion.correctSpring
import dev.morpho.ui.designsystem.motion.floatAnimatable
import dev.morpho.ui.designsystem.motion.runShake
import dev.morpho.ui.designsystem.theme.MorphoTheme

/** Aspect ratio of a quiz image cell, matching the shipped 768x576 media. */
private const val QUIZ_IMAGE_ASPECT = 4f / 3f

/** One cell of an image-based quiz. */
data class ImageOption(
    val wordId: Long,
    val imageFile: String,
    /** Mode 2 only: a one-line English definition under the picture. */
    val caption: String? = null,
    val accessibilityLabel: String,
)

/**
 * Mode-1 stimulus grid: 2x2 images, no text.
 *
 * States and motion come straight from the contract: press scales to 0.97 in 100 ms,
 * a correct pick springs a blue ring plus a check badge and fades the others to 0.4,
 * a wrong pick shakes +/-8dp. Every cell keeps a >=48dp touch target and a content
 * description so TalkBack reads the grid in visual order.
 */
@Composable
fun QuizImageGrid(
    options: List<ImageOption>,
    onSelect: (Int) -> Unit,
    modifier: Modifier = Modifier,
    selectedIndex: Int? = null,
    correctIndex: Int? = null,
    revealed: Boolean = false,
    enabled: Boolean = true,
) {
    QuizGridScaffold(
        options = options,
        modifier = modifier,
        selectedIndex = selectedIndex,
        correctIndex = correctIndex,
        revealed = revealed,
        enabled = enabled,
        onSelect = onSelect,
        showCaptions = false,
    )
}

/**
 * Mode-2 stimulus grid: the same 2x2 images, each carrying a one-line serif
 * definition caption.
 */
@Composable
fun QuizImageDefGrid(
    options: List<ImageOption>,
    onSelect: (Int) -> Unit,
    modifier: Modifier = Modifier,
    selectedIndex: Int? = null,
    correctIndex: Int? = null,
    revealed: Boolean = false,
    enabled: Boolean = true,
) {
    QuizGridScaffold(
        options = options,
        modifier = modifier,
        selectedIndex = selectedIndex,
        correctIndex = correctIndex,
        revealed = revealed,
        enabled = enabled,
        onSelect = onSelect,
        showCaptions = true,
    )
}

@Composable
private fun QuizGridScaffold(
    options: List<ImageOption>,
    modifier: Modifier,
    selectedIndex: Int?,
    correctIndex: Int?,
    revealed: Boolean,
    enabled: Boolean,
    onSelect: (Int) -> Unit,
    showCaptions: Boolean,
) {
    val gutter = MorphoTheme.spacing.sm
    Column(
        modifier = modifier.fillMaxWidth(),
        verticalArrangement = Arrangement.spacedBy(gutter),
    ) {
        for (row in options.indices.chunked(2)) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(gutter),
            ) {
                for (index in row) {
                    QuizImageCell(
                        option = options[index],
                        state = optionState(index, selectedIndex, correctIndex, revealed),
                        showCaption = showCaptions,
                        enabled = enabled,
                        onClick = { onSelect(index) },
                        modifier = Modifier.weight(1f),
                    )
                }
                if (row.size == 1) Box(Modifier.weight(1f))
            }
        }
    }
}

internal fun optionState(
    index: Int,
    selectedIndex: Int?,
    correctIndex: Int?,
    revealed: Boolean,
): QuizOptionState = when {
    !revealed -> if (index == selectedIndex) QuizOptionState.PRESSED else QuizOptionState.IDLE
    index == selectedIndex && index == correctIndex -> QuizOptionState.CORRECT
    index == selectedIndex -> QuizOptionState.WRONG
    index == correctIndex -> QuizOptionState.REVEALED
    else -> QuizOptionState.DIMMED
}

@Composable
private fun QuizImageCell(
    option: ImageOption,
    state: QuizOptionState,
    showCaption: Boolean,
    enabled: Boolean,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val tokens = MorphoTheme.tokens
    val accents = MorphoTheme.accents
    val reducedMotion = MorphoTheme.reducedMotion
    val interactionSource = remember { MutableInteractionSource() }
    val pressed by interactionSource.collectIsPressedAsState()

    val shakeAmplitude = with(LocalDensity.current) { tokens.sizes.shakeAmplitude.toPx() }
    val shake = remember { floatAnimatable() }
    LaunchedEffect(state) {
        if (state == QuizOptionState.WRONG && !reducedMotion) {
            shake.runShake(shakeAmplitude, tokens.durations.shake)
        }
    }

    val ringProgress by animateFloatAsState(
        targetValue = if (state == QuizOptionState.CORRECT || state == QuizOptionState.REVEALED) 1f else 0f,
        animationSpec = correctSpring(),
        label = "ring",
    )
    val targetAlpha = when (state) {
        QuizOptionState.DIMMED -> DIMMED_OPTION_ALPHA
        QuizOptionState.WRONG -> 0.75f
        else -> 1f
    }
    val alpha by animateFloatAsState(
        targetValue = targetAlpha,
        animationSpec = androidx.compose.animation.core.tween(
            tokens.durations.correct,
            easing = tokens.easings.standard,
        ),
        label = "alpha",
    )
    val pressScale by animateFloatAsState(
        targetValue = if (pressed && !reducedMotion) 0.97f else 1f,
        animationSpec = androidx.compose.animation.core.tween(
            tokens.durations.press,
            easing = tokens.easings.standard,
        ),
        label = "press",
    )

    val ringColor = when (state) {
        QuizOptionState.CORRECT, QuizOptionState.REVEALED -> MaterialTheme.colorScheme.primary
        QuizOptionState.WRONG -> accents.wrong
        else -> Color.Transparent
    }

    Column(
        modifier = modifier
            .graphicsLayer {
                translationX = shake.value
                scaleX = pressScale
                scaleY = pressScale
                this.alpha = if (reducedMotion && pressed) 0.75f else alpha
            }
            .clip(tokens.radii.shapeMd)
            .background(MaterialTheme.colorScheme.surfaceContainer)
            .border(
                width = tokens.sizes.optionRingWidth * ringProgress,
                color = if (state == QuizOptionState.WRONG) ringColor else ringColor.copy(alpha = ringProgress),
                shape = tokens.radii.shapeMd,
            )
            .clickable(
                enabled = enabled,
                interactionSource = interactionSource,
                indication = null,
                onClick = onClick,
            )
            .semantics { contentDescription = option.accessibilityLabel },
    ) {
        Box(
            Modifier
                .fillMaxWidth()
                // Release images are 768x576; holding 4:3 keeps the 2x2 grid square-ish
                // and stops a cell from eating the rest of the screen.
                .aspectRatio(QUIZ_IMAGE_ASPECT)
                .heightIn(min = tokens.sizes.quizCellMinHeight),
        ) {
            ContentImage(
                file = option.imageFile,
                contentDescription = null,
                modifier = Modifier.fillMaxSize(),
            )
            val badgeCorrect = state == QuizOptionState.CORRECT || state == QuizOptionState.REVEALED
            val badgeWrong = state == QuizOptionState.WRONG
            if (badgeCorrect || badgeWrong) {
                Box(
                    Modifier
                        .align(Alignment.TopEnd)
                        .padding(tokens.spacing.xs),
                ) {
                    SpringBadge(correct = badgeCorrect)
                }
            }
        }
        if (showCaption && option.caption != null) {
            Text(
                text = option.caption,
                style = MorphoTheme.reading.definitionCaption,
                color = MaterialTheme.colorScheme.onSurface,
                textAlign = TextAlign.Start,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(tokens.spacing.sm),
            )
        }
    }
}

/** Check or cross badge that springs into place, per the motion spec. */
@Composable
private fun SpringBadge(correct: Boolean) {
    val accents = MorphoTheme.accents
    val scale by animateFloatAsState(
        targetValue = 1f,
        animationSpec = correctSpring(),
        label = "badge",
    )
    Box(
        Modifier
            .graphicsLayer {
                scaleX = scale
                scaleY = scale
            }
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
            modifier = Modifier
                .size(18.dp)
                .alpha(1f),
        )
    }
}

private val previewOptions = listOf(
    ImageOption(1, "img/aa11.webp", "kind and generous in spirit", "Option 1"),
    ImageOption(2, "img/bb22.webp", "eager to cause trouble", "Option 2"),
    ImageOption(3, "img/cc33.webp", "unwilling to spend money", "Option 3"),
    ImageOption(4, "img/dd44.webp", "showing careful attention", "Option 4"),
)

@ThemePreviews
@Composable
private fun QuizImageGridIdlePreview() {
    PreviewBox {
        QuizImageGrid(options = previewOptions, onSelect = {})
    }
}

@ThemePreviews
@Composable
private fun QuizImageGridCorrectPreview() {
    PreviewBox {
        QuizImageGrid(
            options = previewOptions,
            onSelect = {},
            selectedIndex = 0,
            correctIndex = 0,
            revealed = true,
        )
    }
}

@ThemePreviews
@Composable
private fun QuizImageGridWrongPreview() {
    PreviewBox {
        QuizImageGrid(
            options = previewOptions,
            onSelect = {},
            selectedIndex = 1,
            correctIndex = 3,
            revealed = true,
        )
    }
}

@ThemePreviews
@Composable
private fun QuizImageDefGridPreview() {
    PreviewBox {
        QuizImageDefGrid(options = previewOptions, onSelect = {})
    }
}
