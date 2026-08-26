package dev.morpho.ui.designsystem.component

import androidx.compose.foundation.background
import androidx.compose.runtime.Composable
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import kotlin.math.abs

/**
 * Indirection between components and the image pipeline.
 *
 * Components only know a content-addressed filename (`img/{hash}.webp`). The app
 * provides a Coil-backed renderer that resolves it through `ContentStore`;
 * `@Preview` and unit tests fall back to [GradientContentImageRenderer], which paints
 * a deterministic gradient derived from the filename. That keeps every component
 * previewable with zero assets and zero network.
 */
interface ContentImageRenderer {
    @Composable
    fun Image(file: String, contentDescription: String?, modifier: Modifier)
}

/** Deterministic colour-gradient stand-in for a real photograph. */
object GradientContentImageRenderer : ContentImageRenderer {
    @Composable
    override fun Image(file: String, contentDescription: String?, modifier: Modifier) {
        val (a, b) = gradientColorsFor(file)
        androidx.compose.foundation.layout.Box(
            modifier
                .background(Brush.linearGradient(listOf(a, b)))
                .semantics { contentDescription?.let { this.contentDescription = it } },
        )
    }
}

/**
 * Two distinct, pleasant colours derived from a filename. Only [GradientContentImageRenderer]
 * uses it, so it is what `@Preview` and component tests paint instead of a photograph.
 */
fun gradientColorsFor(key: String): Pair<Color, Color> {
    var hash = -0x340d631b7bdddcdbL // FNV-1a 64 offset basis
    for (ch in key) {
        hash = hash xor ch.code.toLong()
        hash *= 0x100000001B3L
    }
    val positive = hash and Long.MAX_VALUE
    // Golden-angle hue stepping: consecutive hashes land far apart on the wheel, so
    // the four cells of a grid never read as four shades of the same colour.
    val hue = ((positive % 1_000L) * GOLDEN_ANGLE_DEGREES % 360.0).toFloat()
    val hue2 = (hue + 26f + (positive ushr 17) % 34L) % 360f
    val sat = 0.52f + ((positive ushr 29) % 26L) / 100f
    return hsvToColor(hue, sat, 0.88f) to hsvToColor(hue2, sat * 0.94f, 0.55f)
}

/** 360 / phi^2 - the classic low-discrepancy hue step. */
private const val GOLDEN_ANGLE_DEGREES = 137.50776405003785

private fun hsvToColor(h: Float, s: Float, v: Float): Color {
    val c = v * s
    val x = c * (1 - abs((h / 60f) % 2 - 1))
    val m = v - c
    val (r, g, b) = when {
        h < 60 -> Triple(c, x, 0f)
        h < 120 -> Triple(x, c, 0f)
        h < 180 -> Triple(0f, c, x)
        h < 240 -> Triple(0f, x, c)
        h < 300 -> Triple(x, 0f, c)
        else -> Triple(c, 0f, x)
    }
    return Color(r + m, g + m, b + m)
}

val LocalContentImageRenderer =
    staticCompositionLocalOf<ContentImageRenderer> { GradientContentImageRenderer }

/** What every component calls to paint a content image. */
@Composable
fun ContentImage(file: String, contentDescription: String?, modifier: Modifier = Modifier) {
    LocalContentImageRenderer.current.Image(file, contentDescription, modifier)
}
