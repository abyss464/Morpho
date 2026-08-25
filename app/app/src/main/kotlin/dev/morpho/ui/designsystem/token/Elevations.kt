package dev.morpho.ui.designsystem.token

import androidx.compose.runtime.Immutable
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp

/** Elevation steps. Quiz surfaces stay flat; only floating chrome lifts. */
@Immutable
data class Elevations(
    val flat: Dp = 0.dp,
    val raised: Dp = 1.dp,
    val card: Dp = 3.dp,
    val floating: Dp = 6.dp,
    val sheet: Dp = 8.dp,
)
