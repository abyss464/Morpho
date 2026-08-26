package dev.morpho.ui.home

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.MenuBook
import androidx.compose.material.icons.rounded.Refresh
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ElevatedCard
import androidx.compose.material3.Icon
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import dev.morpho.R
import dev.morpho.domain.progress.TodayProgress
import dev.morpho.ui.designsystem.component.PreviewBox
import dev.morpho.ui.designsystem.component.ThemePreviews
import dev.morpho.ui.designsystem.theme.MorphoTheme

@Composable
fun ActionCard(
    today: TodayProgress,
    hasContent: Boolean,
    onStartLearning: () -> Unit,
    onStartReview: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val spacing = MorphoTheme.spacing

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
                text = stringResource(R.string.home_today_title).uppercase(),
                style = MaterialTheme.typography.titleSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )

            if (!hasContent) {
                Text(
                    text = stringResource(R.string.home_empty),
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                return@Column
            }

            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(spacing.sm),
            ) {
                MetricBox(
                    icon = Icons.Rounded.Refresh,
                    label = stringResource(R.string.home_reviews_due),
                    value = today.dueReviews.toString(),
                    tint = MaterialTheme.colorScheme.primary,
                    dimmed = today.dueReviews == 0,
                    modifier = Modifier.weight(1f),
                )
                MetricBox(
                    icon = Icons.AutoMirrored.Rounded.MenuBook,
                    label = stringResource(R.string.home_new_words),
                    value = "${today.newLearned} / ${today.dailyGoal}",
                    tint = MaterialTheme.colorScheme.secondary,
                    dimmed = false,
                    modifier = Modifier.weight(1f),
                )
            }

            LinearProgressIndicator(
                progress = { today.fraction },
                modifier = Modifier
                    .fillMaxWidth()
                    .heightIn(min = MorphoTheme.sizes.groupBarHeight),
                trackColor = MorphoTheme.accents.ringTrack,
                strokeCap = StrokeCap.Round,
            )

            if (!today.hasWork) {
                Text(
                    text = stringResource(R.string.home_all_done),
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.secondary,
                )
            }

            if (today.dueReviews > 0) {
                Button(
                    onClick = onStartReview,
                    modifier = Modifier
                        .fillMaxWidth()
                        .heightIn(min = spacing.minTouchTarget),
                    shape = MorphoTheme.radii.shapeLg,
                    colors = ButtonDefaults.buttonColors(),
                ) {
                    Text(stringResource(R.string.home_start_review))
                }
                OutlinedButton(
                    onClick = onStartLearning,
                    modifier = Modifier
                        .fillMaxWidth()
                        .heightIn(min = spacing.minTouchTarget),
                    shape = MorphoTheme.radii.shapeLg,
                ) {
                    Text(stringResource(R.string.home_start_learning))
                }
            } else {
                Button(
                    onClick = onStartLearning,
                    modifier = Modifier
                        .fillMaxWidth()
                        .heightIn(min = spacing.minTouchTarget),
                    shape = MorphoTheme.radii.shapeLg,
                    enabled = today.hasWork,
                ) {
                    Text(stringResource(R.string.home_start_learning))
                }
            }
        }
    }
}

@Composable
private fun MetricBox(
    icon: ImageVector,
    label: String,
    value: String,
    tint: androidx.compose.ui.graphics.Color,
    dimmed: Boolean,
    modifier: Modifier = Modifier,
) {
    val alpha = if (dimmed) 0.38f else 1f
    Box(
        modifier = modifier
            .clip(MorphoTheme.radii.shapeMd)
            .background(tint.copy(alpha = 0.08f))
            .padding(MorphoTheme.spacing.md),
    ) {
        Column(verticalArrangement = Arrangement.spacedBy(MorphoTheme.spacing.xs)) {
            Icon(
                imageVector = icon,
                contentDescription = null,
                tint = tint.copy(alpha = alpha),
                modifier = Modifier.size(20.dp),
            )
            Text(
                text = label,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = alpha),
            )
            Text(
                text = value,
                style = MaterialTheme.typography.titleMedium,
                fontWeight = FontWeight.Bold,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = alpha),
            )
        }
    }
}

@ThemePreviews
@Composable
private fun ActionCardPreview() {
    PreviewBox {
        ActionCard(
            today = TodayProgress(
                newLearned = 20,
                dailyGoal = 50,
                reviewed = 8,
                dueReviews = 12,
                correctCount = 23,
                answerCount = 25,
            ),
            hasContent = true,
            onStartLearning = {},
            onStartReview = {},
        )
    }
}

@ThemePreviews
@Composable
private fun ActionCardNoDuePreview() {
    PreviewBox {
        ActionCard(
            today = TodayProgress(
                newLearned = 20,
                dailyGoal = 50,
                reviewed = 0,
                dueReviews = 0,
                correctCount = 20,
                answerCount = 20,
            ),
            hasContent = true,
            onStartLearning = {},
            onStartReview = {},
        )
    }
}
