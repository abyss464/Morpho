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
import androidx.compose.ui.res.pluralStringResource
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
 * diamonds: copper once passed, an outline while still ahead. Under it, the unit the
 * learner is in and the days left at the daily pace, until every word is met.
 */
@Composable
fun JourneyProgress(
    overall: OverallProgress,
    pace: JourneyPace,
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
                    R.string.cd_journey,
                    formatCount(overall.learnedWords),
                    formatCount(overall.totalWords),
                ),
            )

            if (overall.remainingWords > 0) {
                val days = (overall.remainingWords + pace.newPerDay - 1) / pace.newPerDay.coerceAtLeast(1)
                Text(
                    text = stringResource(
                        R.string.today_journey_pace,
                        pace.unit,
                        pace.unitCount,
                        pluralStringResource(R.plurals.days, days, days),
                        pace.newPerDay,
                    ),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
    }
}

/** Where the learner stands: the first unit not fully in review, of all units, at this many new words a day. */
data class JourneyPace(val unit: Int, val unitCount: Int, val newPerDay: Int)

@ThemePreviews
@Composable
private fun JourneyProgressPreview() {
    PreviewBox {
        JourneyProgress(
            overall = OverallProgress(totalWords = 3909, learnedWords = 212, inFlightWords = 3),
            pace = JourneyPace(unit = 11, unitCount = 196, newPerDay = 20),
        )
    }
}
