package dev.morpho.ui.designsystem.token

import androidx.compose.runtime.Immutable
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp

/** Fixed component dimensions, so no screen carries a bare dp literal. */
@Immutable
data class Sizes(
    val audioChip: Dp = 44.dp,
    val audioChipSmall: Dp = 36.dp,
    val modePip: Dp = 10.dp,
    val modePipActive: Dp = 14.dp,
    val progressRing: Dp = 200.dp,
    val progressRingStroke: Dp = 14.dp,
    val groupBarHeight: Dp = 6.dp,
    val quizCellMinHeight: Dp = 132.dp,
    /**
     * Floor for the image band of a bottom-anchored grid. Below this the picture stops
     * carrying the meaning, so a cramped screen scrolls its prompt instead of shrinking
     * the answer further.
     */
    val quizImageMinBand: Dp = 96.dp,
    val spellBox: Dp = 40.dp,
    val spellBoxTall: Dp = 52.dp,
    val checkBadge: Dp = 28.dp,
    val optionRingWidth: Dp = 3.dp,
    val shakeAmplitude: Dp = 8.dp,
    val sharedAxisSlide: Dp = 30.dp,
    val streakBadge: Dp = 40.dp,
    val etymologyChip: Dp = 36.dp,
)
