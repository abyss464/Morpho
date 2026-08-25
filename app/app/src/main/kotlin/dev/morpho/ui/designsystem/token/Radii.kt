package dev.morpho.ui.designsystem.token

import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Immutable
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp

/** Corner radii: 12 / 16 / 24 per docs/contracts/app-design.md. */
@Immutable
data class Radii(
    /** 8dp - chips, pills, small badges. */
    val xs: Dp = 8.dp,
    /** 12dp - option cards, inputs. */
    val sm: Dp = 12.dp,
    /** 16dp - cards, image cells. */
    val md: Dp = 16.dp,
    /** 24dp - sheets, hero surfaces, primary buttons. */
    val lg: Dp = 24.dp,
    /** Fully rounded. */
    val full: Dp = 999.dp,
) {
    val shapeXs = RoundedCornerShape(xs)
    val shapeSm = RoundedCornerShape(sm)
    val shapeMd = RoundedCornerShape(md)
    val shapeLg = RoundedCornerShape(lg)
    val shapeFull = RoundedCornerShape(full)
}
