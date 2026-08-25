package dev.morpho.ui.designsystem.theme

import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.ui.graphics.Color

/**
 * Morpho brand palette: electric morpho blue on deep neutrals
 * (docs/contracts/app-design.md, "Brand").
 *
 * A **static** branded scheme, not Material You dynamic colour — the butterfly blue
 * is the product's identity and must survive on every device. Tonal roles are
 * derived from the three brand seeds:
 *
 *   primary   #2461FF  morpho blue
 *   secondary #00B8A9  teal, correctness accents
 *   error     #E5484D
 */
object MorphoPalette {

    // --- Primary: morpho blue tonal ramp ---
    val Blue10 = Color(0xFF00164A)
    val Blue20 = Color(0xFF002676)
    val Blue30 = Color(0xFF0B3AA8)
    val Blue40 = Color(0xFF2461FF) // brand primary
    val Blue60 = Color(0xFF6E93FF)
    val Blue80 = Color(0xFFB4C6FF)
    val Blue90 = Color(0xFFDCE3FF)
    val Blue95 = Color(0xFFEFF1FF)

    // --- Secondary: teal ---
    val Teal10 = Color(0xFF00201D)
    val Teal20 = Color(0xFF00382F)
    val Teal30 = Color(0xFF008576)
    val Teal40 = Color(0xFF00B8A9) // brand secondary
    val Teal80 = Color(0xFF6FF4E1)
    val Teal90 = Color(0xFFB6FFF2)

    // --- Tertiary: morpho wing violet, used sparingly for celebration ---
    val Violet20 = Color(0xFF32105E)
    val Violet30 = Color(0xFF4B2683)
    val Violet40 = Color(0xFF6B3FB5)
    val Violet80 = Color(0xFFD5BBFF)
    val Violet90 = Color(0xFFEDDCFF)

    // --- Error ---
    val Red30 = Color(0xFF8C1A1E)
    val Red40 = Color(0xFFE5484D) // brand error
    val Red80 = Color(0xFFFFB3B2)
    val Red90 = Color(0xFFFFDAD8)

    // --- Deep neutrals ---
    val Neutral6 = Color(0xFF0C0E14)
    val Neutral10 = Color(0xFF12151C)
    val Neutral15 = Color(0xFF1B1F28)
    val Neutral20 = Color(0xFF232833)
    val Neutral25 = Color(0xFF2C323F)
    val Neutral30 = Color(0xFF3A4150)
    val Neutral60 = Color(0xFF8A91A0)
    val Neutral80 = Color(0xFFC5CAD6)
    val Neutral90 = Color(0xFFE2E5EC)
    val Neutral95 = Color(0xFFF1F3F7)
    val Neutral98 = Color(0xFFFAFBFD)
    val White = Color(0xFFFFFFFF)

    // --- Neutral variants (outlines, surface variants) ---
    val NeutralVariant30 = Color(0xFF434A5C)
    val NeutralVariant50 = Color(0xFF737B8F)
    val NeutralVariant60 = Color(0xFF8D95A9)
    val NeutralVariant80 = Color(0xFFC3C8D8)
    val NeutralVariant90 = Color(0xFFDFE2EF)
}

val MorphoLightColorScheme = lightColorScheme(
    primary = MorphoPalette.Blue40,
    onPrimary = MorphoPalette.White,
    primaryContainer = MorphoPalette.Blue90,
    onPrimaryContainer = MorphoPalette.Blue10,
    inversePrimary = MorphoPalette.Blue80,

    secondary = MorphoPalette.Teal30,
    onSecondary = MorphoPalette.White,
    secondaryContainer = MorphoPalette.Teal90,
    onSecondaryContainer = MorphoPalette.Teal10,

    tertiary = MorphoPalette.Violet40,
    onTertiary = MorphoPalette.White,
    tertiaryContainer = MorphoPalette.Violet90,
    onTertiaryContainer = MorphoPalette.Violet20,

    error = MorphoPalette.Red40,
    onError = MorphoPalette.White,
    errorContainer = MorphoPalette.Red90,
    onErrorContainer = MorphoPalette.Red30,

    background = MorphoPalette.Neutral98,
    onBackground = MorphoPalette.Neutral10,
    surface = MorphoPalette.Neutral98,
    onSurface = MorphoPalette.Neutral10,
    surfaceVariant = MorphoPalette.NeutralVariant90,
    onSurfaceVariant = MorphoPalette.NeutralVariant30,
    surfaceTint = MorphoPalette.Blue40,
    inverseSurface = MorphoPalette.Neutral20,
    inverseOnSurface = MorphoPalette.Neutral95,

    surfaceContainerLowest = MorphoPalette.White,
    surfaceContainerLow = MorphoPalette.Neutral98,
    surfaceContainer = MorphoPalette.Neutral95,
    surfaceContainerHigh = MorphoPalette.Neutral90,
    surfaceContainerHighest = Color(0xFFDDE1EA),

    outline = MorphoPalette.NeutralVariant50,
    outlineVariant = MorphoPalette.NeutralVariant80,
    scrim = Color(0xFF000000),
)

val MorphoDarkColorScheme = darkColorScheme(
    primary = MorphoPalette.Blue60,
    onPrimary = MorphoPalette.Blue10,
    primaryContainer = MorphoPalette.Blue30,
    onPrimaryContainer = MorphoPalette.Blue90,
    inversePrimary = MorphoPalette.Blue40,

    secondary = MorphoPalette.Teal40,
    onSecondary = MorphoPalette.Teal10,
    secondaryContainer = MorphoPalette.Teal20,
    onSecondaryContainer = MorphoPalette.Teal80,

    tertiary = MorphoPalette.Violet80,
    onTertiary = MorphoPalette.Violet20,
    tertiaryContainer = MorphoPalette.Violet30,
    onTertiaryContainer = MorphoPalette.Violet90,

    error = Color(0xFFFF6E70),
    onError = Color(0xFF4E0206),
    errorContainer = MorphoPalette.Red30,
    onErrorContainer = MorphoPalette.Red90,

    background = MorphoPalette.Neutral6,
    onBackground = MorphoPalette.Neutral90,
    surface = MorphoPalette.Neutral6,
    onSurface = MorphoPalette.Neutral90,
    surfaceVariant = MorphoPalette.Neutral25,
    onSurfaceVariant = MorphoPalette.NeutralVariant80,
    surfaceTint = MorphoPalette.Blue60,
    inverseSurface = MorphoPalette.Neutral90,
    inverseOnSurface = MorphoPalette.Neutral15,

    surfaceContainerLowest = Color(0xFF070910),
    surfaceContainerLow = MorphoPalette.Neutral10,
    surfaceContainer = MorphoPalette.Neutral15,
    surfaceContainerHigh = MorphoPalette.Neutral20,
    surfaceContainerHighest = MorphoPalette.Neutral25,

    outline = MorphoPalette.NeutralVariant60,
    outlineVariant = MorphoPalette.Neutral30,
    scrim = Color(0xFF000000),
)

/**
 * Semantic colours that have no Material role but are part of the brand system.
 * Reached through `MorphoTheme.accents`.
 */
@androidx.compose.runtime.Immutable
data class MorphoAccents(
    val correct: Color,
    val onCorrect: Color,
    val correctContainer: Color,
    val wrong: Color,
    val wrongContainer: Color,
    val streak: Color,
    val highlight: Color,
    val onHighlight: Color,
    val ringTrack: Color,
    val modePipInactive: Color,
    val shimmer: Color,
)

val LightAccents = MorphoAccents(
    correct = MorphoPalette.Teal30,
    onCorrect = MorphoPalette.White,
    correctContainer = MorphoPalette.Teal90,
    wrong = MorphoPalette.Red40,
    wrongContainer = MorphoPalette.Red90,
    streak = Color(0xFFF7A325),
    highlight = MorphoPalette.Blue90,
    onHighlight = MorphoPalette.Blue20,
    ringTrack = MorphoPalette.NeutralVariant90,
    modePipInactive = MorphoPalette.NeutralVariant80,
    shimmer = Color(0x33FFFFFF),
)

val DarkAccents = MorphoAccents(
    correct = MorphoPalette.Teal40,
    onCorrect = MorphoPalette.Teal10,
    correctContainer = MorphoPalette.Teal20,
    wrong = Color(0xFFFF6E70),
    wrongContainer = MorphoPalette.Red30,
    streak = Color(0xFFFFC163),
    highlight = MorphoPalette.Blue30,
    onHighlight = MorphoPalette.Blue90,
    ringTrack = MorphoPalette.Neutral25,
    modePipInactive = MorphoPalette.Neutral30,
    shimmer = Color(0x1FFFFFFF),
)
