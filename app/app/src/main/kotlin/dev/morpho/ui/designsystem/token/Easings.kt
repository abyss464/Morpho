package dev.morpho.ui.designsystem.token

import androidx.compose.animation.core.CubicBezierEasing
import androidx.compose.animation.core.Easing
import androidx.compose.animation.core.LinearEasing
import androidx.compose.runtime.Immutable

/** M3 motion easing set; `standard` is the contract's CubicBezier(0.2, 0, 0, 1). */
@Immutable
data class Easings(
    val standard: Easing = CubicBezierEasing(0.2f, 0f, 0f, 1f),
    val standardDecelerate: Easing = CubicBezierEasing(0f, 0f, 0f, 1f),
    val standardAccelerate: Easing = CubicBezierEasing(0.3f, 0f, 1f, 1f),
    val emphasized: Easing = CubicBezierEasing(0.2f, 0f, 0f, 1f),
    val linear: Easing = LinearEasing,
)
