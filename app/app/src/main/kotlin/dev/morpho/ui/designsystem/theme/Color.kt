package dev.morpho.ui.designsystem.theme

import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.ui.graphics.Color

/**
 * Morpho brand palette, derived from the app icon (`res/drawable/ic_launcher_foreground.xml`).
 *
 * Four seeds, and nothing outside them:
 *
 *   ink blue    #22314A  the icon's field — light-theme text, dark-theme ground
 *   copper gold #C08F4A  the icon's active diamond — the single accent
 *   parchment   #F2EDE2  the icon's page — light-theme ground, dark-theme text
 *   mist blue   #6B7893  the icon's resting square — the quiet secondary
 *
 * A **static** branded scheme, not Material You dynamic colour: the icon is the
 * product's identity and must survive on every device (docs/contracts/app-design.md,
 * "Brand"). Each seed carries a tonal ramp so Material's roles resolve without
 * inventing hues that are not on the icon.
 */
object MorphoPalette {

    // --- Ink blue: the icon's field. Primary in light, ground in dark. ---
    val Ink05 = Color(0xFF0B111B)
    val Ink10 = Color(0xFF131C2A)
    val Ink15 = Color(0xFF1A2536)
    val Ink20 = Color(0xFF22314A) // icon background, brand ink
    val Ink25 = Color(0xFF2A3A57)
    val Ink30 = Color(0xFF35496E)
    val Ink40 = Color(0xFF43567A)
    val Ink50 = Color(0xFF5B7099)
    val Ink60 = Color(0xFF7B90B5)
    val Ink70 = Color(0xFFA9BDDC)
    val Ink80 = Color(0xFFC3D1E5)
    val Ink90 = Color(0xFFD9E1EE)
    val Ink95 = Color(0xFFECF0F6)

    // --- Mist blue: the icon's resting square. Secondary. ---
    val Mist20 = Color(0xFF2B3345)
    val Mist30 = Color(0xFF414C64)
    val Mist35 = Color(0xFF4A5570)
    val Mist40 = Color(0xFF6B7893) // brand mist
    /** Lifted mist, so a resting mark still clears 3:1 against the ink ground. */
    val Mist50 = Color(0xFF7E8CA6)
    val Mist60 = Color(0xFF9AA6BE)
    val Mist80 = Color(0xFFC6CDDA)
    val Mist90 = Color(0xFFDFE4EC)

    // --- Copper gold: the icon's active diamond. The one accent. ---
    val Copper20 = Color(0xFF3E2B12)
    val Copper25 = Color(0xFF4A3316)
    val Copper30 = Color(0xFF6E4C22)
    val Copper40 = Color(0xFF8F6329)
    /** Deepened copper, so the frontier diamond still clears 3:1 on a parchment page. */
    val Copper45 = Color(0xFFB07C33)
    val Copper50 = Color(0xFFC08F4A) // brand copper
    val Copper70 = Color(0xFFD9B384)
    val Copper80 = Color(0xFFE7CBA6)
    val Copper90 = Color(0xFFF4E4CD)

    // --- Parchment: the icon's page. Ground in light, text in dark. ---
    val Parchment70 = Color(0xFFC9BEA6)
    val Parchment80 = Color(0xFFDED5C2)
    val Parchment85 = Color(0xFFE9E2D3)
    val Parchment90 = Color(0xFFF2EDE2) // brand parchment
    val Parchment95 = Color(0xFFF8F4EB)
    val Parchment98 = Color(0xFFFCFAF4)
    val Parchment100 = Color(0xFFFFFDF8)

    /**
     * Verdigris — aged-copper green. The "correct" signal, kept in the icon's
     * metal family rather than a stock UI green.
     */
    val Verdigris20 = Color(0xFF12312A)
    val Verdigris30 = Color(0xFF255045)
    val Verdigris40 = Color(0xFF3B7A67)
    val Verdigris60 = Color(0xFF6FAD98)
    val Verdigris80 = Color(0xFFB5D8C9)
    val Verdigris90 = Color(0xFFD8ECE3)

    /** Oxblood — the bookbinder's red. The "wrong" and error signal. */
    val Oxblood20 = Color(0xFF3F1210)
    val Oxblood30 = Color(0xFF75211E)
    val Oxblood40 = Color(0xFF9E322D)
    val Oxblood60 = Color(0xFFC96A63)
    val Oxblood80 = Color(0xFFEDB2AB)
    val Oxblood90 = Color(0xFFF8DBD5)
}

/**
 * Light: a parchment page, ink-blue type, copper for anything the eye should land on.
 * Cards sit *above* the page (lighter) the way a card sits on a desk.
 */
val MorphoLightColorScheme = lightColorScheme(
    primary = MorphoPalette.Ink20,
    onPrimary = MorphoPalette.Parchment95,
    primaryContainer = MorphoPalette.Ink90,
    onPrimaryContainer = MorphoPalette.Ink10,
    inversePrimary = MorphoPalette.Ink70,

    secondary = MorphoPalette.Mist35,
    onSecondary = MorphoPalette.Parchment95,
    secondaryContainer = MorphoPalette.Mist90,
    onSecondaryContainer = MorphoPalette.Mist20,

    tertiary = MorphoPalette.Copper40,
    onTertiary = MorphoPalette.Parchment98,
    tertiaryContainer = MorphoPalette.Copper90,
    onTertiaryContainer = MorphoPalette.Copper25,

    error = MorphoPalette.Oxblood40,
    onError = MorphoPalette.Parchment98,
    errorContainer = MorphoPalette.Oxblood90,
    onErrorContainer = MorphoPalette.Oxblood30,

    background = MorphoPalette.Parchment90,
    onBackground = MorphoPalette.Ink10,
    surface = MorphoPalette.Parchment90,
    onSurface = MorphoPalette.Ink10,
    surfaceVariant = MorphoPalette.Parchment85,
    onSurfaceVariant = MorphoPalette.Mist30,
    surfaceTint = MorphoPalette.Copper50,
    inverseSurface = MorphoPalette.Ink20,
    inverseOnSurface = MorphoPalette.Parchment90,

    surfaceContainerLowest = MorphoPalette.Parchment100,
    surfaceContainerLow = MorphoPalette.Parchment98,
    surfaceContainer = MorphoPalette.Parchment95,
    surfaceContainerHigh = MorphoPalette.Parchment85,
    surfaceContainerHighest = MorphoPalette.Parchment80,

    outline = MorphoPalette.Mist40,
    outlineVariant = MorphoPalette.Parchment70,
    scrim = Color(0xCC131C2A),
)

/**
 * Dark: the icon itself, full-bleed. Ink-blue ground, parchment type, the same
 * copper diamond doing the same job.
 */
val MorphoDarkColorScheme = darkColorScheme(
    primary = MorphoPalette.Ink70,
    onPrimary = MorphoPalette.Ink10,
    primaryContainer = MorphoPalette.Ink30,
    onPrimaryContainer = MorphoPalette.Ink90,
    inversePrimary = MorphoPalette.Ink20,

    secondary = MorphoPalette.Mist60,
    onSecondary = MorphoPalette.Mist20,
    secondaryContainer = MorphoPalette.Mist30,
    onSecondaryContainer = MorphoPalette.Mist90,

    tertiary = MorphoPalette.Copper50,
    onTertiary = MorphoPalette.Copper20,
    tertiaryContainer = MorphoPalette.Copper30,
    onTertiaryContainer = MorphoPalette.Copper90,

    error = MorphoPalette.Oxblood60,
    onError = MorphoPalette.Oxblood20,
    errorContainer = MorphoPalette.Oxblood30,
    onErrorContainer = MorphoPalette.Oxblood90,

    background = MorphoPalette.Ink20,
    onBackground = MorphoPalette.Parchment90,
    surface = MorphoPalette.Ink20,
    onSurface = MorphoPalette.Parchment90,
    surfaceVariant = MorphoPalette.Ink25,
    onSurfaceVariant = MorphoPalette.Mist80,
    surfaceTint = MorphoPalette.Copper50,
    inverseSurface = MorphoPalette.Parchment90,
    inverseOnSurface = MorphoPalette.Ink20,

    surfaceContainerLowest = MorphoPalette.Ink15,
    surfaceContainerLow = MorphoPalette.Ink25,
    surfaceContainer = MorphoPalette.Ink30,
    surfaceContainerHigh = MorphoPalette.Ink40,
    surfaceContainerHighest = MorphoPalette.Ink50,

    outline = MorphoPalette.Mist40,
    outlineVariant = MorphoPalette.Ink40,
    scrim = Color(0xCC0B111B),
)

/**
 * Semantic colours that have no Material role but are part of the brand system.
 * Reached through `MorphoTheme.accents`.
 *
 * [motifBase], [motifActive] and [motifMastered] are the three marks of the icon's
 * progress row — mist-blue square, copper diamond, parchment diamonds — and they
 * read, in that order, as *untouched → learning → mastered*. In light theme the
 * mastered mark flips to ink so it survives on a parchment page.
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
    /** The icon's resting square: not started. */
    val motifBase: Color,
    /** The icon's copper diamond: in progress, and the one thing the eye lands on. */
    val motifActive: Color,
    /** The icon's trailing diamonds: done. */
    val motifMastered: Color,
    /** Hairline ornament rules between sections. */
    val rule: Color,
)

val LightAccents = MorphoAccents(
    correct = MorphoPalette.Verdigris40,
    onCorrect = MorphoPalette.Parchment98,
    correctContainer = MorphoPalette.Verdigris90,
    wrong = MorphoPalette.Oxblood40,
    wrongContainer = MorphoPalette.Oxblood90,
    streak = MorphoPalette.Copper40,
    highlight = MorphoPalette.Copper90,
    onHighlight = MorphoPalette.Copper25,
    ringTrack = MorphoPalette.Parchment80,
    modePipInactive = MorphoPalette.Parchment70,
    shimmer = Color(0x33FFFFFF),
    motifBase = MorphoPalette.Mist40,
    motifActive = MorphoPalette.Copper45,
    motifMastered = MorphoPalette.Ink20,
    rule = MorphoPalette.Parchment70,
)

val DarkAccents = MorphoAccents(
    correct = MorphoPalette.Verdigris60,
    onCorrect = MorphoPalette.Verdigris20,
    correctContainer = MorphoPalette.Verdigris30,
    wrong = MorphoPalette.Oxblood60,
    wrongContainer = MorphoPalette.Oxblood30,
    streak = MorphoPalette.Copper50,
    highlight = MorphoPalette.Copper30,
    onHighlight = MorphoPalette.Copper90,
    ringTrack = MorphoPalette.Ink30,
    modePipInactive = MorphoPalette.Ink40,
    shimmer = Color(0x1FFFFFFF),
    motifBase = MorphoPalette.Mist50,
    motifActive = MorphoPalette.Copper50,
    motifMastered = MorphoPalette.Parchment90,
    rule = MorphoPalette.Ink40,
)
