package dev.morpho.ui.designsystem.motion

import androidx.compose.animation.ContentTransform
import androidx.compose.animation.EnterTransition
import androidx.compose.animation.ExitTransition
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.AnimationVector1D
import androidx.compose.animation.core.Easing
import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.SpringSpec
import androidx.compose.animation.core.VectorConverter
import androidx.compose.animation.core.keyframes
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.animation.togetherWith
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalDensity
import dev.morpho.ui.designsystem.theme.MorphoTheme

/**
 * Shared motion vocabulary. Every duration and easing comes from the token layer, and
 * every helper degrades to a plain cross-fade under reduced motion.
 *
 * The transition builders are deliberately **not** `@Composable`: both
 * `AnimatedContent.transitionSpec` and `NavHost`'s enter/exit lambdas run outside
 * composition, so the values they need are captured once via [rememberSharedAxis].
 */
@Immutable
data class SharedAxis(
    val durationMs: Int,
    val easing: Easing,
    val slidePx: Int,
    val reducedMotion: Boolean,
) {
    /** Shared-axis X: slide 30dp + fade, 300 ms (app-design.md motion table). */
    fun transform(forward: Boolean = true): ContentTransform =
        enter(forward) togetherWith exit(forward)

    fun enter(forward: Boolean): EnterTransition {
        val fade = fadeIn(tween(durationMs, easing = easing))
        if (reducedMotion) return fade
        val sign = if (forward) 1 else -1
        return slideInHorizontally(tween(durationMs, easing = easing)) { sign * slidePx } + fade
    }

    fun exit(forward: Boolean): ExitTransition {
        val fade = fadeOut(tween(durationMs / 2, easing = easing))
        if (reducedMotion) return fade
        val sign = if (forward) 1 else -1
        return slideOutHorizontally(tween(durationMs, easing = easing)) { -sign * slidePx } + fade
    }
}

@Composable
fun rememberSharedAxis(): SharedAxis {
    val duration = MorphoTheme.durations.transition
    val easing = MorphoTheme.easings.standard
    val reduced = MorphoTheme.reducedMotion
    val slide = with(LocalDensity.current) { MorphoTheme.sizes.sharedAxisSlide.roundToPx() }
    return remember(duration, easing, reduced, slide) {
        SharedAxis(duration, easing, slide, reduced)
    }
}

/** Press feedback: scale 1 -> 0.97; opacity instead when motion is reduced. */
fun Modifier.pressScale(pressed: Boolean, reducedMotion: Boolean, scale: Float = 0.97f): Modifier =
    graphicsLayer {
        if (reducedMotion) {
            alpha = if (pressed) 0.75f else 1f
        } else {
            val s = if (pressed) scale else 1f
            scaleX = s
            scaleY = s
        }
    }

/** Spring used for the correct-answer ring and check badge. */
fun <T> correctSpring(): SpringSpec<T> = spring(
    dampingRatio = Spring.DampingRatioMediumBouncy,
    stiffness = Spring.StiffnessMediumLow,
)

/**
 * Horizontal shake, +/-8dp over 300 ms. Keyframed so the motion reads as a rejection
 * rather than a wobble.
 */
suspend fun Animatable<Float, *>.runShake(amplitudePx: Float, durationMs: Int) {
    snapTo(0f)
    animateTo(
        targetValue = 0f,
        animationSpec = keyframes {
            durationMillis = durationMs
            0f at 0
            amplitudePx at durationMs / 6
            -amplitudePx at durationMs * 2 / 6
            amplitudePx * 0.6f at durationMs * 3 / 6
            -amplitudePx * 0.4f at durationMs * 4 / 6
            amplitudePx * 0.2f at durationMs * 5 / 6
            0f at durationMs
        },
    )
}

/** Convenience factory so callers do not have to spell out the converter. */
fun floatAnimatable(initial: Float = 0f): Animatable<Float, AnimationVector1D> =
    Animatable(initial, Float.VectorConverter)
