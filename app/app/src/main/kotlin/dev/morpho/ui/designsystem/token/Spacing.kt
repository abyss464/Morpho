package dev.morpho.ui.designsystem.token

import androidx.compose.runtime.Immutable
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp

/**
 * 4dp spacing grid (docs/contracts/app-design.md, "Tokens").
 * Screens must reference these instead of literal dp values.
 */
@Immutable
data class Spacing(
    val none: Dp = 0.dp,
    /** 4dp - hairline gaps inside a component. */
    val xxs: Dp = 4.dp,
    /** 8dp - between tightly related elements. */
    val xs: Dp = 8.dp,
    /** 12dp - inner padding of small components. */
    val sm: Dp = 12.dp,
    /** 16dp - default screen gutter and card padding. */
    val md: Dp = 16.dp,
    /** 20dp - grid gutter between quiz cells. */
    val lg: Dp = 20.dp,
    /** 24dp - between sections. */
    val xl: Dp = 24.dp,
    /** 32dp - above a primary call to action. */
    val xxl: Dp = 32.dp,
    /** 48dp - large hero breathing room. */
    val xxxl: Dp = 48.dp,
) {
    /** Minimum touch target mandated by the accessibility section of the contract. */
    val minTouchTarget: Dp = 48.dp

    /** Default horizontal screen gutter. */
    val screenGutter: Dp = md
}
