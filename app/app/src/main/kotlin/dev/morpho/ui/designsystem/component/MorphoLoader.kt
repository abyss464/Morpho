package dev.morpho.ui.designsystem.component

import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.StartOffset
import androidx.compose.animation.core.StartOffsetType
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.keyframes
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.RectangleShape
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.unit.dp
import dev.morpho.ui.designsystem.theme.MorphoTheme

/**
 * The four small marks from the app icon (mist-blue square, copper diamond, two
 * parchment diamonds), reused here as a compact, reusable loading indicator.
 *
 * Each mark scales up and fades in on its own delay, so the wave visibly flows
 * left to right, then loops. Under reduced motion the scale is dropped and only
 * the fade plays (docs/contracts/app-design.md, "Respect reduced-motion").
 */
@Composable
fun MorphoLoader(modifier: Modifier = Modifier) {
    val reducedMotion = MorphoTheme.reducedMotion
    val durations = MorphoTheme.durations
    val easing = MorphoTheme.easings.standard

    // One full pulse - fade/scale up then back down - per mark.
    val pulseDurationMs = durations.correct * 2
    // Delay between one mark starting its pulse and the next, so the wave flows.
    val staggerMs = durations.press

    val transition = rememberInfiniteTransition(label = "morpho-loader")
    val markSpecs = rememberMarkSpecs()

    Row(
        modifier = modifier.clearAndSetSemantics { contentDescription = "Loading" },
        horizontalArrangement = Arrangement.spacedBy(MorphoTheme.spacing.xxs),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        markSpecs.forEachIndexed { index, spec ->
            val progress by transition.animateFloat(
                initialValue = 0f,
                targetValue = 0f,
                animationSpec = infiniteRepeatable(
                    animation = keyframes {
                        durationMillis = pulseDurationMs
                        0f at 0 using easing
                        1f at pulseDurationMs / 2 using easing
                        0f at pulseDurationMs
                    },
                    repeatMode = RepeatMode.Restart,
                    initialStartOffset = StartOffset(index * staggerMs, StartOffsetType.Delay),
                ),
                label = "morpho-loader-mark-$index",
            )

            val scale = if (reducedMotion) 1f else lerpFloat(MarkMinScale, MarkMaxScale, progress)
            val alpha = lerpFloat(MarkMinAlpha, 1f, progress)

            Box(
                modifier = Modifier
                    .size(MarkSize)
                    .graphicsLayer {
                        scaleX = scale
                        scaleY = scale
                        this.alpha = alpha
                        rotationZ = spec.rotationDegrees
                    }
                    .background(spec.color, spec.shape),
            )
        }
    }
}

private fun lerpFloat(start: Float, stop: Float, fraction: Float): Float =
    start + (stop - start) * fraction

private data class MorphoLoaderMark(
    val color: Color,
    val shape: Shape,
    val rotationDegrees: Float = 0f,
)

private val MarkSize = 10.dp
private const val MarkMinScale = 0.55f
private const val MarkMaxScale = 1.15f
private const val MarkMinAlpha = 0.3f

private val SquareShape = RoundedCornerShape(percent = 15)

/**
 * The icon's four marks, read from the theme rather than hard-coded: the third and
 * fourth are parchment on the icon's ink field, which would vanish on a parchment page,
 * so `motifMastered` flips to ink in the light scheme.
 */
@Composable
private fun rememberMarkSpecs(): List<MorphoLoaderMark> {
    val accents = MorphoTheme.accents
    return remember(accents) {
        listOf(
            MorphoLoaderMark(color = accents.motifBase, shape = SquareShape),
            MorphoLoaderMark(
                color = accents.motifActive,
                shape = RectangleShape,
                rotationDegrees = 45f,
            ),
            MorphoLoaderMark(
                color = accents.motifMastered,
                shape = RectangleShape,
                rotationDegrees = 45f,
            ),
            MorphoLoaderMark(
                color = accents.motifMastered,
                shape = RectangleShape,
                rotationDegrees = 45f,
            ),
        )
    }
}

@ThemePreviews
@Composable
private fun MorphoLoaderPreview() {
    PreviewBox {
        MorphoLoader()
    }
}
