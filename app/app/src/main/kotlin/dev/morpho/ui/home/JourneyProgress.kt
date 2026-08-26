package dev.morpho.ui.home

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ElevatedCard
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.Fill
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import dev.morpho.R
import dev.morpho.domain.progress.OverallProgress
import dev.morpho.ui.designsystem.component.PreviewBox
import dev.morpho.ui.designsystem.component.ThemePreviews
import dev.morpho.ui.designsystem.theme.MorphoTheme

@Composable
fun JourneyProgress(
    overall: OverallProgress,
    estimatedDaysRemaining: Int?,
    modifier: Modifier = Modifier,
) {
    val spacing = MorphoTheme.spacing
    val primary = MaterialTheme.colorScheme.primary
    val tertiary = MaterialTheme.colorScheme.tertiary
    val trackColor = MorphoTheme.accents.ringTrack
    val cardBg = MaterialTheme.colorScheme.surfaceContainerLow
    val percentComplete = (overall.fraction * 100).toInt()

    ElevatedCard(
        modifier = modifier.fillMaxWidth(),
        shape = MorphoTheme.radii.shapeLg,
        colors = CardDefaults.elevatedCardColors(
            containerColor = MaterialTheme.colorScheme.surfaceContainerLow,
        ),
        elevation = CardDefaults.elevatedCardElevation(
            defaultElevation = MorphoTheme.elevations.card,
        ),
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(spacing.xl),
            verticalArrangement = Arrangement.spacedBy(spacing.md),
        ) {
            Text(
                text = stringResource(R.string.home_section_journey),
                style = MaterialTheme.typography.titleSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )

            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.Bottom,
            ) {
                Text(
                    text = formatCount(overall.learnedWords),
                    style = MaterialTheme.typography.titleLarge,
                    fontWeight = FontWeight.Bold,
                    color = MaterialTheme.colorScheme.onSurface,
                )
                Text(
                    text = stringResource(
                        R.string.home_journey_of,
                        formatCount(overall.totalWords),
                    ),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }

            val milestones = listOf(0.25f, 0.50f, 0.75f)
            Canvas(
                modifier = Modifier
                    .fillMaxWidth()
                    .height(12.dp),
            ) {
                val barHeight = 8.dp.toPx()
                val barY = (size.height - barHeight) / 2f
                val corner = CornerRadius(barHeight / 2f)
                val dotRadius = 5.dp.toPx()

                drawRoundRect(
                    color = trackColor,
                    topLeft = Offset(0f, barY),
                    size = Size(size.width, barHeight),
                    cornerRadius = corner,
                )

                val fillWidth = size.width * overall.fraction.coerceIn(0f, 1f)
                if (fillWidth > 0f) {
                    drawRoundRect(
                        brush = Brush.horizontalGradient(
                            colors = listOf(primary, tertiary),
                            startX = 0f,
                            endX = size.width,
                        ),
                        topLeft = Offset(0f, barY),
                        size = Size(fillWidth, barHeight),
                        cornerRadius = corner,
                    )
                }

                val centerY = size.height / 2f
                milestones.forEach { milestone ->
                    val x = size.width * milestone
                    if (overall.fraction >= milestone) {
                        drawCircle(
                            color = Color.White,
                            radius = dotRadius,
                            center = Offset(x, centerY),
                            style = Fill,
                        )
                        drawCircle(
                            color = primary,
                            radius = dotRadius - 1.5.dp.toPx(),
                            center = Offset(x, centerY),
                            style = Fill,
                        )
                    } else {
                        drawCircle(
                            color = trackColor,
                            radius = dotRadius,
                            center = Offset(x, centerY),
                            style = Fill,
                        )
                        drawCircle(
                            color = cardBg,
                            radius = dotRadius - 1.5.dp.toPx(),
                            center = Offset(x, centerY),
                            style = Fill,
                        )
                    }
                }
            }

            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
            ) {
                Text(
                    text = stringResource(R.string.home_journey_complete, percentComplete),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                when {
                    overall.remainingWords == 0 -> Text(
                        text = stringResource(R.string.home_journey_done),
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.primary,
                    )
                    estimatedDaysRemaining != null -> Text(
                        text = stringResource(R.string.home_journey_days_left, estimatedDaysRemaining),
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
        }
    }
}

@ThemePreviews
@Composable
private fun JourneyProgressPreview() {
    PreviewBox {
        JourneyProgress(
            overall = OverallProgress(totalWords = 4253, learnedWords = 2090, inFlightWords = 17),
            estimatedDaysRemaining = 43,
        )
    }
}
