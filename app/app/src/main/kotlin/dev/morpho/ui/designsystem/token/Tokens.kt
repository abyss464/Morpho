package dev.morpho.ui.designsystem.token

import androidx.compose.runtime.Immutable
import androidx.compose.runtime.staticCompositionLocalOf

/**
 * The single token bundle screens read from. Reached through
 * `MorphoTheme.spacing`, `MorphoTheme.durations`, etc.
 */
@Immutable
data class MorphoTokens(
    val spacing: Spacing = Spacing(),
    val radii: Radii = Radii(),
    val elevations: Elevations = Elevations(),
    val durations: Durations = Durations(),
    val easings: Easings = Easings(),
    val sizes: Sizes = Sizes(),
)

val LocalMorphoTokens = staticCompositionLocalOf { MorphoTokens() }
