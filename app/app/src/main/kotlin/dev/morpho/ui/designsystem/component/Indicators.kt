package dev.morpho.ui.designsystem.component

import androidx.compose.animation.core.animateDpAsState
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import dev.morpho.ui.designsystem.theme.MorphoTheme

/**
 * Three-dot indicator for the mode ladder (1 -> 2 -> 3). The active pip grows and
 * fills; a promotion animates the fill over 400 ms with a soft glow behind it.
 */
@Composable
fun ModePips(
    mode: Int,
    modifier: Modifier = Modifier,
    total: Int = 3,
) {
    val accents = MorphoTheme.accents
    val duration = MorphoTheme.durations.promote
    Row(
        modifier = modifier.clearAndSetSemantics {
            contentDescription = "Learning mode $mode of $total"
        },
        horizontalArrangement = Arrangement.spacedBy(MorphoTheme.spacing.xs),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        for (level in 1..total) {
            val reached = level <= mode
            val isActive = level == mode
            val size by animateDpAsState(
                targetValue = if (isActive) MorphoTheme.sizes.modePipActive else MorphoTheme.sizes.modePip,
                animationSpec = tween(duration, easing = MorphoTheme.easings.standard),
                label = "pip-size",
            )
            val fill by animateFloatAsState(
                targetValue = if (reached) 1f else 0f,
                animationSpec = tween(duration, easing = MorphoTheme.easings.standard),
                label = "pip-fill",
            )
            Box(contentAlignment = Alignment.Center) {
                if (isActive) {
                    Box(
                        Modifier
                            .size(size + MorphoTheme.spacing.xs)
                            .background(
                                MaterialTheme.colorScheme.primary.copy(alpha = 0.18f * fill),
                                CircleShape,
                            ),
                    )
                }
                Box(
                    Modifier
                        .size(size)
                        .background(
                            color = androidx.compose.ui.graphics.lerp(
                                accents.modePipInactive,
                                MaterialTheme.colorScheme.primary,
                                fill,
                            ),
                            shape = CircleShape,
                        ),
                )
            }
        }
    }
}

/** Per-word state used to colour one segment of the group bar. */
enum class GroupSegmentState { PENDING, IN_PROGRESS, PASSED, CARRIED }

/**
 * Segmented progress across the words in the current learning unit — one segment per
 * word, so the user can see exactly how much of the group is left.
 */
@Composable
fun GroupProgressBar(
    segments: List<GroupSegmentState>,
    modifier: Modifier = Modifier,
) {
    val accents = MorphoTheme.accents
    val passed = segments.count { it == GroupSegmentState.PASSED }
    Row(
        modifier = modifier
            .fillMaxWidth()
            .height(MorphoTheme.sizes.groupBarHeight)
            .clearAndSetSemantics {
                contentDescription = "$passed of ${segments.size} words completed in this group"
            },
        horizontalArrangement = Arrangement.spacedBy(2.dp),
    ) {
        segments.forEach { segment ->
            val color by animateFloatAsState(
                targetValue = when (segment) {
                    GroupSegmentState.PASSED -> 1f
                    GroupSegmentState.IN_PROGRESS -> 0.55f
                    GroupSegmentState.CARRIED -> 0.35f
                    GroupSegmentState.PENDING -> 0f
                },
                animationSpec = tween(MorphoTheme.durations.correct),
                label = "segment",
            )
            val base = when (segment) {
                GroupSegmentState.CARRIED -> MorphoTheme.accents.streak
                else -> MaterialTheme.colorScheme.primary
            }
            Box(
                Modifier
                    .weight(1f)
                    .height(MorphoTheme.sizes.groupBarHeight)
                    .clip(MorphoTheme.radii.shapeFull)
                    .background(
                        androidx.compose.ui.graphics.lerp(accents.ringTrack, base, color),
                    ),
            )
        }
    }
}

/**
 * Home-screen progress ring: an animated 600 ms decelerating sweep with the headline
 * stat in the middle.
 */
@Composable
fun ProgressRing(
    progress: Float,
    centerLabel: String,
    centerCaption: String,
    modifier: Modifier = Modifier,
    accessibilityLabel: String = "$centerLabel $centerCaption",
) {
    val accents = MorphoTheme.accents
    val primary = MaterialTheme.colorScheme.primary
    val secondary = MaterialTheme.colorScheme.tertiary
    val stroke = MorphoTheme.sizes.progressRingStroke
    val animated by animateFloatAsState(
        targetValue = progress.coerceIn(0f, 1f),
        animationSpec = tween(
            durationMillis = MorphoTheme.durations.progressRing,
            easing = MorphoTheme.easings.standardDecelerate,
        ),
        label = "ring-sweep",
    )

    Box(
        modifier = modifier
            .size(MorphoTheme.sizes.progressRing)
            .clearAndSetSemantics { contentDescription = accessibilityLabel },
        contentAlignment = Alignment.Center,
    ) {
        Canvas(Modifier.size(MorphoTheme.sizes.progressRing)) {
            val strokePx = stroke.toPx()
            val inset = strokePx / 2f
            val arcSize = Size(size.width - strokePx, size.height - strokePx)
            drawArc(
                color = accents.ringTrack,
                startAngle = -90f,
                sweepAngle = 360f,
                useCenter = false,
                topLeft = Offset(inset, inset),
                size = arcSize,
                style = Stroke(width = strokePx, cap = StrokeCap.Round),
            )
            if (animated > 0f) {
                drawArc(
                    brush = androidx.compose.ui.graphics.Brush.sweepGradient(
                        listOf(primary, secondary, primary),
                    ),
                    startAngle = -90f,
                    sweepAngle = 360f * animated,
                    useCenter = false,
                    topLeft = Offset(inset, inset),
                    size = arcSize,
                    style = Stroke(width = strokePx, cap = StrokeCap.Round),
                )
            }
        }
        Column(horizontalAlignment = Alignment.CenterHorizontally) {
            Text(
                text = centerLabel,
                style = MorphoTheme.reading.statNumber,
                color = MaterialTheme.colorScheme.onSurface,
            )
            Text(
                text = centerCaption,
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                textAlign = TextAlign.Center,
            )
        }
    }
}

@ThemePreviews
@Composable
private fun ModePipsPreview() {
    PreviewBox {
        Column(verticalArrangement = Arrangement.spacedBy(MorphoTheme.spacing.md)) {
            ModePips(mode = 1)
            ModePips(mode = 2)
            ModePips(mode = 3)
        }
    }
}

@ThemePreviews
@Composable
private fun GroupProgressBarPreview() {
    PreviewBox {
        Column(verticalArrangement = Arrangement.spacedBy(MorphoTheme.spacing.md)) {
            GroupProgressBar(
                segments = List(6) { GroupSegmentState.PASSED } +
                    listOf(GroupSegmentState.IN_PROGRESS) +
                    List(2) { GroupSegmentState.CARRIED } +
                    List(9) { GroupSegmentState.PENDING },
            )
            GroupProgressBar(segments = List(18) { GroupSegmentState.PENDING })
        }
    }
}

@ThemePreviews
@Composable
private fun ProgressRingPreview() {
    PreviewBox {
        Box(Modifier.padding(MorphoTheme.spacing.md), contentAlignment = Alignment.Center) {
            ProgressRing(
                progress = 0.38f,
                centerLabel = "2,090",
                centerCaption = "of 5,500 learned",
            )
        }
    }
}

/** Kept next to the ring so the two share the same colour maths. */
internal fun Color.blendWith(other: Color, fraction: Float): Color =
    androidx.compose.ui.graphics.lerp(this, other, fraction)
