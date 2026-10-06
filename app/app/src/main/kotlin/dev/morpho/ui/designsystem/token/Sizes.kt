package dev.morpho.ui.designsystem.token

import androidx.compose.runtime.Immutable
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp

/** Fixed component dimensions, so no screen carries a bare dp literal. */
@Immutable
data class Sizes(
    val audioChip: Dp = 44.dp,
    val audioChipSmall: Dp = 36.dp,
    val shakeAmplitude: Dp = 8.dp,
    val sharedAxisSlide: Dp = 30.dp,
    /** Height of a primary pill button (Continue, Start). */
    val primaryButton: Dp = 52.dp,
    /** Thin progress tracks: the journey and unit bars. */
    val trackHeight: Dp = 4.dp,
    /** Today's progress bar at the top of the stream. */
    val streamBarHeight: Dp = 6.dp,
    /** A word option in the use step. */
    val wordOption: Dp = 56.dp,
    /** The picture beside "What does … mean?" in the first explain step. */
    val thumbnailWidth: Dp = 72.dp,
    val thumbnailHeight: Dp = 54.dp,
    /** The explain step's tray, so it reads as a place to build in before anything is in it. */
    val trayMinHeight: Dp = 150.dp,
    /** The covered picture of a review question. */
    val coverHeight: Dp = 160.dp,
    /** The picture on a review result. */
    val resultPictureHeight: Dp = 190.dp,
)
