package dev.morpho.ui.designsystem.token

import androidx.compose.runtime.Immutable

/**
 * Motion durations in milliseconds, straight from the motion spec table in
 * docs/contracts/app-design.md.
 */
@Immutable
data class Durations(
    /** Press feedback: scale 1 -> 0.97; also the loader's stagger. */
    val press: Int = 100,
    /** Half of one loader pulse: the mark scales up, then back down. */
    val pulse: Int = 250,
    /** Wrong pick or failed check: horizontal shake. */
    val shake: Int = 300,
    /** Step and screen transition, shared axis X. */
    val transition: Int = 300,
)
