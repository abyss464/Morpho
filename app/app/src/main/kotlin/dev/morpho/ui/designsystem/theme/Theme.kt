package dev.morpho.ui.designsystem.theme

import android.provider.Settings
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Shapes
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.ReadOnlyComposable
import androidx.compose.runtime.remember
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.platform.LocalContext
import dev.morpho.ui.designsystem.token.Durations
import dev.morpho.ui.designsystem.token.Easings
import dev.morpho.ui.designsystem.token.Elevations
import dev.morpho.ui.designsystem.token.LocalMorphoTokens
import dev.morpho.ui.designsystem.token.MorphoTokens
import dev.morpho.ui.designsystem.token.Radii
import dev.morpho.ui.designsystem.token.Sizes
import dev.morpho.ui.designsystem.token.Spacing

val LocalMorphoAccents = staticCompositionLocalOf { LightAccents }
val LocalReadingTypography = staticCompositionLocalOf { MorphoReadingTypography() }

/**
 * True when the platform asks for reduced motion. Screens replace scale/shake with
 * opacity when this is set (docs/contracts/app-design.md, "Respect reduced-motion").
 */
val LocalReducedMotion = staticCompositionLocalOf { false }

/**
 * The app theme. Dark is first class: both schemes ship in wave 1 and every
 * component preview is rendered in both.
 *
 * Deliberately a **static** branded scheme — no Material You dynamic colour, because
 * morpho blue is the product identity (docs/contracts/app-design.md, "Brand").
 */
@Composable
fun MorphoTheme(
    darkTheme: Boolean = isSystemInDarkTheme(),
    tokens: MorphoTokens = MorphoTokens(),
    forceReducedMotion: Boolean? = null,
    content: @Composable () -> Unit,
) {
    val colorScheme = if (darkTheme) MorphoDarkColorScheme else MorphoLightColorScheme
    val accents = if (darkTheme) DarkAccents else LightAccents

    val reducedMotion = forceReducedMotion ?: rememberSystemReducedMotion()

    val shapes = Shapes(
        extraSmall = tokens.radii.shapeXs,
        small = tokens.radii.shapeSm,
        medium = tokens.radii.shapeMd,
        large = tokens.radii.shapeLg,
        extraLarge = tokens.radii.shapeLg,
    )

    CompositionLocalProvider(
        LocalMorphoTokens provides tokens,
        LocalMorphoAccents provides accents,
        LocalReadingTypography provides MorphoReadingTypography(),
        LocalReducedMotion provides reducedMotion,
    ) {
        MaterialTheme(
            colorScheme = colorScheme,
            typography = MorphoTypography,
            shapes = shapes,
            content = content,
        )
    }
}

/**
 * Reads the platform's "remove animations" accessibility setting.
 *
 * Compose has no first-class reduced-motion signal, so this reads
 * `Settings.Global.ANIMATOR_DURATION_SCALE`, which is what the system toggle writes
 * and what the platform itself honours.
 */
@Composable
private fun rememberSystemReducedMotion(): Boolean {
    val context = LocalContext.current
    return remember(context) {
        runCatching {
            Settings.Global.getFloat(
                context.contentResolver,
                Settings.Global.ANIMATOR_DURATION_SCALE,
                1f,
            ) == 0f
        }.getOrDefault(false)
    }
}

/** Token accessors. Screens use `MorphoTheme.spacing.md`, never `16.dp`. */
object MorphoTheme {
    val tokens: MorphoTokens
        @Composable @ReadOnlyComposable get() = LocalMorphoTokens.current

    val spacing: Spacing
        @Composable @ReadOnlyComposable get() = LocalMorphoTokens.current.spacing

    val radii: Radii
        @Composable @ReadOnlyComposable get() = LocalMorphoTokens.current.radii

    val elevations: Elevations
        @Composable @ReadOnlyComposable get() = LocalMorphoTokens.current.elevations

    val durations: Durations
        @Composable @ReadOnlyComposable get() = LocalMorphoTokens.current.durations

    val easings: Easings
        @Composable @ReadOnlyComposable get() = LocalMorphoTokens.current.easings

    val sizes: Sizes
        @Composable @ReadOnlyComposable get() = LocalMorphoTokens.current.sizes

    val accents: MorphoAccents
        @Composable @ReadOnlyComposable get() = LocalMorphoAccents.current

    val reading: MorphoReadingTypography
        @Composable @ReadOnlyComposable get() = LocalReadingTypography.current

    val reducedMotion: Boolean
        @Composable @ReadOnlyComposable get() = LocalReducedMotion.current
}
