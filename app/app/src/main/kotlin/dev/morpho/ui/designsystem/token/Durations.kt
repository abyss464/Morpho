package dev.morpho.ui.designsystem.token

import androidx.compose.runtime.Immutable

/**
 * Motion durations in milliseconds, straight from the motion spec table in
 * docs/contracts/app-design.md.
 */
@Immutable
data class Durations(
    /** Option press: scale 1 -> 0.97. */
    val press: Int = 100,
    /** Red flash on the answer feedback overlay. */
    val flash: Int = 150,
    /** Correct answer ring + check badge. */
    val correct: Int = 250,
    /** Wrong answer horizontal shake. */
    val shake: Int = 300,
    /** Question transition, shared axis X. */
    val transition: Int = 300,
    /** Mode promotion pip fill + glow. */
    val promote: Int = 400,
    /** Home progress ring sweep. */
    val progressRing: Int = 600,
    /** Group-complete celebration ceiling. */
    val celebration: Int = 1500,
    /** Generic short fade for supporting content. */
    val fade: Int = 200,
)
