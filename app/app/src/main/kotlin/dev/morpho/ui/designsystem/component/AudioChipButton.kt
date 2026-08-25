package dev.morpho.ui.designsystem.component

import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.sizeIn
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.GraphicEq
import androidx.compose.material.icons.rounded.VolumeUp
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.draw.scale
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import dev.morpho.ui.designsystem.theme.MorphoTheme

/**
 * Play button for a single content-audio file.
 *
 * While audio is playing it grows a pulsing ring, so the user can see which of the
 * several audio affordances on a detail sheet is currently speaking. The pulse is
 * suppressed under reduced motion; the icon change still carries the state.
 */
@Composable
fun AudioChipButton(
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    playing: Boolean = false,
    enabled: Boolean = true,
    small: Boolean = false,
    contentDescription: String = "Play pronunciation",
) {
    val size = if (small) MorphoTheme.sizes.audioChipSmall else MorphoTheme.sizes.audioChip
    val ringColor = MaterialTheme.colorScheme.primary
    val reducedMotion = MorphoTheme.reducedMotion

    val pulse = if (playing && !reducedMotion) {
        val transition = rememberInfiniteTransition(label = "audio-pulse")
        transition.animateFloat(
            initialValue = 0f,
            targetValue = 1f,
            animationSpec = infiniteRepeatable(
                animation = tween(1100, easing = MorphoTheme.easings.standardDecelerate),
                repeatMode = RepeatMode.Restart,
            ),
            label = "audio-pulse-progress",
        ).value
    } else {
        0f
    }

    IconButton(
        onClick = onClick,
        enabled = enabled,
        modifier = modifier
            .sizeIn(
                minWidth = MorphoTheme.spacing.minTouchTarget,
                minHeight = MorphoTheme.spacing.minTouchTarget,
            )
            .semantics { this.contentDescription = contentDescription },
    ) {
        Box(contentAlignment = Alignment.Center) {
            if (playing && !reducedMotion) {
                Box(
                    Modifier
                        .size(size)
                        .drawBehind {
                            val radius = this.size.minDimension / 2f * (1f + pulse * 0.6f)
                            drawCircle(
                                color = ringColor.copy(alpha = (1f - pulse) * 0.55f),
                                radius = radius,
                                center = Offset(this.size.width / 2f, this.size.height / 2f),
                                style = Stroke(width = 2.dp.toPx()),
                            )
                        },
                )
            }
            Box(
                Modifier
                    .size(size)
                    .background(
                        color = if (playing) {
                            MaterialTheme.colorScheme.primaryContainer
                        } else {
                            MaterialTheme.colorScheme.surfaceContainerHigh
                        },
                        shape = CircleShape,
                    ),
                contentAlignment = Alignment.Center,
            ) {
                Icon(
                    imageVector = if (playing) Icons.Rounded.GraphicEq else Icons.Rounded.VolumeUp,
                    contentDescription = null,
                    tint = if (playing) {
                        MaterialTheme.colorScheme.onPrimaryContainer
                    } else {
                        MaterialTheme.colorScheme.onSurfaceVariant
                    },
                    modifier = Modifier
                        .size(if (small) 18.dp else 22.dp)
                        .scale(if (playing && !reducedMotion) 1f + pulse * 0.08f else 1f),
                )
            }
        }
    }
}

@ThemePreviews
@Composable
private fun AudioChipButtonPreview() {
    PreviewBox {
        Row(horizontalArrangement = Arrangement.spacedBy(MorphoTheme.spacing.md)) {
            AudioChipButton(onClick = {})
            AudioChipButton(onClick = {}, playing = true)
            AudioChipButton(onClick = {}, small = true)
            AudioChipButton(onClick = {}, enabled = false)
        }
    }
}
