package dev.morpho.ui.designsystem.motion

import androidx.compose.animation.ContentTransform
import androidx.compose.animation.EnterTransition
import androidx.compose.animation.ExitTransition
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.AnimationVector1D
import androidx.compose.animation.core.Easing
import androidx.compose.animation.core.VectorConverter
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.keyframes
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.animation.togetherWith
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.waitForUpOrCancellation
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.pointerInput
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

/**
 * Press feedback for anything tappable: scales to 0.97 over the press duration while a
 * finger is down, and back; dims instead when motion is reduced.
 *
 * It watches the pointer itself rather than an interaction source, because `clickable`
 * inside a scrolling container holds its press back until it knows the touch is not a
 * scroll — a quick tap then never shows as pressed. Watching on the initial pass, without
 * consuming, leaves the element's own click and ripple untouched, and a touch that turns
 * into a scroll releases the press.
 */
@Composable
fun Modifier.pressMotion(enabled: Boolean = true): Modifier {
    var pressed by remember { mutableStateOf(false) }
    val reducedMotion = MorphoTheme.reducedMotion
    val shown = pressed && enabled
    val scale by animateFloatAsState(
        targetValue = if (shown && !reducedMotion) PRESSED_SCALE else 1f,
        animationSpec = tween(MorphoTheme.durations.press, easing = MorphoTheme.easings.standard),
        label = "press",
    )
    return this
        .graphicsLayer {
            if (reducedMotion) {
                alpha = if (shown) PRESSED_ALPHA else 1f
            } else {
                scaleX = scale
                scaleY = scale
            }
        }
        .pointerInput(enabled) {
            awaitEachGesture {
                awaitFirstDown(requireUnconsumed = false, pass = PointerEventPass.Initial)
                pressed = true
                waitForUpOrCancellation(pass = PointerEventPass.Initial)
                pressed = false
            }
        }
}

private const val PRESSED_SCALE = 0.97f
private const val PRESSED_ALPHA = 0.75f

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
