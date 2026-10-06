package dev.morpho.ui.stream

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import dev.morpho.R
import dev.morpho.ui.designsystem.component.MarkKind
import dev.morpho.ui.designsystem.component.MorphoMark
import dev.morpho.ui.designsystem.component.PrimaryButton
import dev.morpho.ui.designsystem.component.ScreenPreviewBox
import dev.morpho.ui.designsystem.component.ScreenPreviews
import dev.morpho.ui.designsystem.component.StreakBadge
import dev.morpho.ui.designsystem.theme.MorphoSectionLabel
import dev.morpho.ui.designsystem.theme.MorphoTheme

/**
 * Today's stream is done (docs/contracts/stream.md §7): steps and words, then reviewed
 * (with the clean count), met (with the first-time count), tomorrow's due reviews, the
 * streak and any unit finished today; Done for today, or Meet 5 more words.
 */
@Composable
internal fun DoneView(
    summary: DoneSummary,
    onDone: () -> Unit,
    onMeetMore: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val spacing = MorphoTheme.spacing
    val accents = MorphoTheme.accents
    Column(modifier.fillMaxSize()) {
        Column(
            modifier = Modifier
                .weight(1f)
                .verticalScroll(rememberScrollState())
                .padding(horizontal = spacing.screenGutter)
                .padding(top = spacing.xxxl),
            verticalArrangement = Arrangement.spacedBy(spacing.md),
        ) {
            Text(
                text = stringResource(R.string.stream_done_eyebrow).uppercase(),
                style = MorphoSectionLabel,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            Text(
                text = stringResource(
                    R.string.stream_done_headline,
                    pluralStringResource(R.plurals.steps, summary.steps, summary.steps),
                    pluralStringResource(R.plurals.words, summary.reviewed + summary.met, summary.reviewed + summary.met),
                ),
                style = MaterialTheme.typography.displaySmall,
                color = MaterialTheme.colorScheme.onSurface,
            )
            Column(verticalArrangement = Arrangement.spacedBy(spacing.xs)) {
                DoneTile(
                    mark = MarkKind.DIAMOND,
                    markColor = accents.motifMastered,
                    title = stringResource(R.string.stream_done_reviewed),
                    detail = stringResource(R.string.stream_done_reviewed_detail, summary.reviewedClean),
                    figure = summary.reviewed,
                )
                DoneTile(
                    mark = MarkKind.SQUARE,
                    markColor = accents.motifBase,
                    title = stringResource(R.string.stream_done_met),
                    detail = stringResource(R.string.stream_done_met_detail, summary.metClean),
                    figure = summary.met,
                )
                DoneTile(
                    mark = MarkKind.DIAMOND,
                    markColor = accents.motifActive,
                    title = stringResource(R.string.stream_done_tomorrow),
                    detail = pluralStringResource(R.plurals.stream_done_tomorrow_detail, summary.newPerDay, summary.newPerDay),
                    figure = summary.tomorrow,
                )
            }
            val units = summary.units?.let { unitLine(it) }
            if (summary.streak > 0) {
                StreakBadge(
                    days = summary.streak,
                    label = stringResource(R.string.stream_done_streak),
                )
            }
            val line = listOfNotNull(
                stringResource(R.string.stream_done_no_streak).takeIf { summary.streak == 0 },
                units,
            ).joinToString(" ")
            if (line.isNotEmpty()) {
                Text(
                    text = line,
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
            Spacer(Modifier.height(spacing.md))
        }
        Column(
            modifier = Modifier.padding(horizontal = spacing.screenGutter, vertical = spacing.sm),
            verticalArrangement = Arrangement.spacedBy(spacing.xs),
        ) {
            PrimaryButton(stringResource(R.string.stream_done_for_today), onDone)
            PrimaryButton(stringResource(R.string.stream_meet_more), onMeetMore, outlined = true)
        }
    }
}

/** "Unit 11 is finished; Unit 12 starts tomorrow." and its variants. */
@Composable
private fun unitLine(news: UnitNews): String {
    val last = news.finished.last()
    val done = if (news.finished.size > 1) {
        stringResource(R.string.stream_done_units_finished, news.finished.joinToString(", "))
    } else {
        stringResource(R.string.stream_done_unit_finished, last)
    }
    return when (news.ending) {
        UnitEnding.LAST_UNIT -> stringResource(R.string.stream_done_last_unit, done)
        UnitEnding.NEXT_BEGUN -> stringResource(R.string.stream_done_unit_plain, done)
        UnitEnding.NEXT_TOMORROW -> stringResource(R.string.stream_done_next_unit, done, last + 1)
    }
}

@Composable
private fun DoneTile(
    mark: MarkKind,
    markColor: androidx.compose.ui.graphics.Color,
    title: String,
    detail: String,
    figure: Int,
) {
    val spacing = MorphoTheme.spacing
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clip(MorphoTheme.radii.shapeMd)
            .background(MaterialTheme.colorScheme.surfaceContainerLow)
            .padding(spacing.md)
            .semantics(mergeDescendants = true) {},
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(spacing.sm),
    ) {
        MorphoMark(mark, markColor, size = 10.dp)
        Column(Modifier.weight(1f)) {
            Text(text = title, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
            Text(text = detail, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        Text(text = "$figure", style = MorphoTheme.reading.statNumber, color = MaterialTheme.colorScheme.onSurface)
    }
}

@ScreenPreviews
@Composable
private fun DoneViewPreview() {
    ScreenPreviewBox {
        DoneView(
            summary = DoneSummary(
                steps = 37,
                reviewed = 14,
                reviewedClean = 12,
                met = 20,
                metClean = 17,
                tomorrow = 23,
                newPerDay = 20,
                streak = 13,
                units = UnitNews(listOf(11), UnitEnding.NEXT_TOMORROW),
            ),
            onDone = {},
            onMeetMore = {},
        )
    }
}
