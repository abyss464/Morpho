package dev.morpho.ui.designsystem.component

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.size
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.semantics
import com.airbnb.lottie.compose.LottieAnimation
import com.airbnb.lottie.compose.LottieCompositionSpec
import com.airbnb.lottie.compose.animateLottieCompositionAsState
import com.airbnb.lottie.compose.rememberLottieComposition
import dev.morpho.ui.designsystem.theme.MorphoTheme

/** What the overlay is currently announcing. */
sealed interface FeedbackSignal {
    data object None : FeedbackSignal
    data object Correct : FeedbackSignal
    data object Wrong : FeedbackSignal
    data class Promoted(val mode: Int) : FeedbackSignal
    data class GroupComplete(val message: String) : FeedbackSignal
}

/**
 * Full-screen, non-blocking feedback layer.
 *
 * Per the motion spec: a wrong answer flashes red for 150 ms, a correct answer gets a
 * brief primary wash, and a completed group hosts a celebration (Lottie when a
 * composition is supplied, otherwise a procedural butterfly-blue burst) capped at
 * 1.5 s and skippable by tap.
 */
@Composable
fun AnswerFeedbackOverlay(
    signal: FeedbackSignal,
    onCelebrationFinished: () -> Unit,
    modifier: Modifier = Modifier,
    celebrationAsset: String? = null,
) {
    val tokens = MorphoTheme.tokens
    val accents = MorphoTheme.accents

    val flashColor = when (signal) {
        is FeedbackSignal.Wrong -> accents.wrong
        is FeedbackSignal.Correct -> MaterialTheme.colorScheme.primary
        is FeedbackSignal.Promoted -> MaterialTheme.colorScheme.tertiary
        else -> Color.Transparent
    }
    val flashAlpha by animateFloatAsState(
        targetValue = when (signal) {
            is FeedbackSignal.Wrong -> 0.22f
            is FeedbackSignal.Correct -> 0.12f
            is FeedbackSignal.Promoted -> 0.14f
            else -> 0f
        },
        animationSpec = tween(tokens.durations.flash, easing = tokens.easings.standard),
        label = "flash",
    )

    Box(modifier = modifier.fillMaxSize()) {
        if (flashAlpha > 0f) {
            Box(
                Modifier
                    .fillMaxSize()
                    .drawBehind {
                        drawRect(
                            Brush.radialGradient(
                                colors = listOf(
                                    flashColor.copy(alpha = flashAlpha),
                                    Color.Transparent,
                                ),
                                radius = size.maxDimension * 0.8f,
                            ),
                        )
                    }
                    .semantics {
                        liveRegion = LiveRegionMode.Polite
                        contentDescription = when (signal) {
                            is FeedbackSignal.Wrong -> "Incorrect"
                            is FeedbackSignal.Correct -> "Correct"
                            is FeedbackSignal.Promoted -> "Promoted to mode ${signal.mode}"
                            else -> ""
                        }
                    },
            )
        }

        AnimatedVisibility(
            visible = signal is FeedbackSignal.GroupComplete,
            enter = fadeIn(tween(tokens.durations.fade)),
            exit = fadeOut(tween(tokens.durations.fade)),
            modifier = Modifier.align(Alignment.Center),
        ) {
            val message = (signal as? FeedbackSignal.GroupComplete)?.message.orEmpty()
            Celebration(
                message = message,
                assetName = celebrationAsset,
                onFinished = onCelebrationFinished,
            )
        }
    }
}

@Composable
private fun Celebration(
    message: String,
    assetName: String?,
    onFinished: () -> Unit,
) {
    val tokens = MorphoTheme.tokens
    val interaction = remember { MutableInteractionSource() }

    // Cap at 1.5s regardless of the asset's own length, and allow a tap to skip.
    LaunchedEffect(message) {
        kotlinx.coroutines.delay(tokens.durations.celebration.toLong())
        onFinished()
    }

    Box(
        Modifier
            .fillMaxSize()
            .background(MaterialTheme.colorScheme.scrim.copy(alpha = 0.35f))
            .clickable(interactionSource = interaction, indication = null, onClick = onFinished),
        contentAlignment = Alignment.Center,
    ) {
        Column(
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(tokens.spacing.md),
        ) {
            if (assetName != null) {
                val composition by rememberLottieComposition(
                    LottieCompositionSpec.Asset(assetName),
                )
                val progress by animateLottieCompositionAsState(composition, iterations = 1)
                LottieAnimation(
                    composition = composition,
                    progress = { progress },
                    modifier = Modifier.size(tokens.sizes.progressRing),
                )
            } else {
                WingBurst()
            }
            Text(
                text = message,
                style = MaterialTheme.typography.headlineSmall,
                color = MaterialTheme.colorScheme.inverseOnSurface,
            )
        }
    }
}

/**
 * Procedural stand-in for the butterfly Lottie: concentric morpho-blue wing arcs
 * blooming outward. Ships in wave 1 so the celebration exists without an asset.
 */
@Composable
private fun WingBurst() {
    val tokens = MorphoTheme.tokens
    val primary = MaterialTheme.colorScheme.primary
    val tertiary = MaterialTheme.colorScheme.tertiary
    val bloom by animateFloatAsState(
        targetValue = 1f,
        animationSpec = tween(tokens.durations.celebration, easing = tokens.easings.standardDecelerate),
        label = "bloom",
    )
    Box(
        Modifier
            .size(tokens.sizes.progressRing)
            .drawBehind {
                val cx = size.width / 2f
                val cy = size.height / 2f
                repeat(7) { i ->
                    val phase = (bloom - i * 0.08f).coerceIn(0f, 1f)
                    if (phase <= 0f) return@repeat
                    val radius = size.minDimension * 0.12f + phase * size.minDimension * 0.42f
                    drawCircle(
                        color = androidx.compose.ui.graphics.lerp(primary, tertiary, i / 6f)
                            .copy(alpha = (1f - phase) * 0.55f),
                        radius = radius,
                        center = androidx.compose.ui.geometry.Offset(cx, cy),
                        style = androidx.compose.ui.graphics.drawscope.Stroke(
                            width = size.minDimension * 0.03f,
                        ),
                    )
                }
            },
    )
}

@ThemePreviews
@Composable
private fun AnswerFeedbackOverlayPreview() {
    PreviewBox {
        Box(Modifier.size(320.dp2())) {
            AnswerFeedbackOverlay(
                signal = FeedbackSignal.Wrong,
                onCelebrationFinished = {},
            )
        }
    }
}

private fun Int.dp2() = androidx.compose.ui.unit.Dp(this.toFloat())
