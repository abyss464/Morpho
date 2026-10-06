package dev.morpho.ui.today

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ElevatedCard
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import dev.morpho.R
import dev.morpho.domain.progress.OverallProgress
import dev.morpho.ui.designsystem.component.MotifMilestoneRail
import dev.morpho.ui.designsystem.component.PreviewBox
import dev.morpho.ui.designsystem.component.SectionHeading
import dev.morpho.ui.designsystem.component.ThemePreviews
import dev.morpho.ui.designsystem.theme.MorphoTheme

/**
 * The long arc: how much of the whole release the learner has met.
 *
 * One Garamond figure carries the card — the count of words met — with the total set
 * beside it as a quiet denominator. The quarter marks ride the rail as the icon's
 * diamonds: copper once passed, an outline while still ahead.
 */
@Composable
fun JourneyProgress(
    overall: OverallProgress,
    estimatedDaysRemaining: Int?,
    modifier: Modifier = Modifier,
) {
    val spacing = MorphoTheme.spacing
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
            SectionHeading(stringResource(R.string.today_section_journey))

            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(spacing.xs),
                verticalAlignment = Alignment.Bottom,
            ) {
                Text(
                    text = formatCount(overall.learnedWords),
                    style = MorphoTheme.reading.heroNumber,
                    color = MaterialTheme.colorScheme.onSurface,
                )
                Text(
                    text = stringResource(
                        R.string.today_journey_of,
                        formatCount(overall.totalWords),
                    ),
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.padding(bottom = spacing.xs),
                )
            }

            MotifMilestoneRail(
                fraction = overall.fraction,
                contentDescription = stringResource(
                    R.string.today_journey_complete,
                    percentComplete,
                ),
            )

            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
            ) {
                Text(
                    text = stringResource(R.string.today_journey_complete, percentComplete),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                when {
                    overall.remainingWords == 0 -> Text(
                        text = stringResource(R.string.today_journey_done),
                        style = MaterialTheme.typography.bodySmall,
                        color = MorphoTheme.accents.motifActive,
                    )
                    estimatedDaysRemaining != null -> Text(
                        text = stringResource(
                            R.string.today_journey_days_left,
                            estimatedDaysRemaining,
                        ),
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
