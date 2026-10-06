package dev.morpho.ui.today

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.BarChart
import androidx.compose.material.icons.rounded.GridView
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ElevatedCard
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import dev.morpho.R
import dev.morpho.di.AppContainer
import dev.morpho.domain.model.ActivityChartStyle
import dev.morpho.domain.model.DailyActivity
import dev.morpho.domain.model.GreetingPeriod
import dev.morpho.domain.progress.OverallProgress
import dev.morpho.ui.designsystem.component.MotifOrnament
import dev.morpho.ui.designsystem.component.PrimaryButton
import dev.morpho.ui.designsystem.component.ScreenPreviewBox
import dev.morpho.ui.designsystem.component.SectionHeading
import dev.morpho.ui.designsystem.component.ScreenPreviews
import dev.morpho.ui.designsystem.motion.pressMotion
import dev.morpho.ui.designsystem.theme.MorphoTheme
import java.time.LocalDate

/**
 * Today, the only way into learning (docs/contracts/stream.md §7): one line on what the
 * day holds and one button into the stream, then the journey, this week, "Look up a word"
 * and the units as progress.
 */
@Composable
fun TodayScreen(
    container: AppContainer,
    onOpenStream: () -> Unit,
    onOpenSettings: () -> Unit,
    onOpenWord: (Long) -> Unit,
    modifier: Modifier = Modifier,
) {
    val viewModel: TodayViewModel = viewModel(factory = TodayViewModel.factory(container))
    val state by viewModel.state.collectAsStateWithLifecycle()

    LaunchedEffect(Unit) { viewModel.refresh() }

    TodayContent(
        state = state,
        modifier = modifier.fillMaxSize(),
        onOpenStream = {
            container.tap()
            onOpenStream()
        },
        onOpenSettings = {
            container.tap()
            onOpenSettings()
        },
        onToggleChartStyle = { style ->
            container.tap()
            viewModel.setActivityChartStyle(style)
        },
        onOpenWord = { id ->
            container.tap()
            onOpenWord(id)
        },
    )
}

@Composable
private fun TodayContent(
    state: TodayUiState,
    onOpenStream: () -> Unit,
    onOpenSettings: () -> Unit,
    onToggleChartStyle: (ActivityChartStyle) -> Unit,
    onOpenWord: (Long) -> Unit,
    modifier: Modifier = Modifier,
) {
    val spacing = MorphoTheme.spacing
    Column(
        modifier = modifier
            .background(MaterialTheme.colorScheme.background)
            .statusBarsPadding()
            .imePadding()
            .verticalScroll(rememberScrollState())
            .padding(horizontal = spacing.screenGutter)
            .padding(top = spacing.md, bottom = spacing.xxl),
        verticalArrangement = Arrangement.spacedBy(spacing.lg),
    ) {
        GreetingHeader(
            greetingPeriod = state.greetingPeriod,
            streakDays = state.streakDays,
            onOpenSettings = onOpenSettings,
        )

        TodayCard(state = state, onOpenStream = onOpenStream)

        JourneyProgress(
            overall = state.journey,
            pace = state.pace,
        )

        Column(verticalArrangement = Arrangement.spacedBy(spacing.sm)) {
            SectionHeading(
                text = stringResource(R.string.today_section_this_week),
                trailing = {
                    IconButton(
                        onClick = {
                            onToggleChartStyle(
                                when (state.activityChartStyle) {
                                    ActivityChartStyle.BAR -> ActivityChartStyle.HEATMAP
                                    ActivityChartStyle.HEATMAP -> ActivityChartStyle.BAR
                                },
                            )
                        },
                        modifier = Modifier
                            .size(spacing.minTouchTarget)
                            .pressMotion(),
                    ) {
                        Icon(
                            imageVector = when (state.activityChartStyle) {
                                ActivityChartStyle.BAR -> Icons.Rounded.GridView
                                ActivityChartStyle.HEATMAP -> Icons.Rounded.BarChart
                            },
                            contentDescription = stringResource(R.string.settings_activity_chart),
                            tint = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                },
            )
            when (state.activityChartStyle) {
                ActivityChartStyle.BAR -> WeeklyBarChart(activity = state.weeklyActivity)
                ActivityChartStyle.HEATMAP -> ActivityHeatmap(data = state.heatmapData)
            }
        }

        WordSearch(index = state.index, onPick = onOpenWord)

        if (state.units.isNotEmpty()) {
            UnitsSection(state)
        }

        MotifOrnament()
    }
}

/** "Today", the day's reviews and new words in one line, and the way into the stream. */
@Composable
private fun TodayCard(state: TodayUiState, onOpenStream: () -> Unit) {
    val spacing = MorphoTheme.spacing
    ElevatedCard(
        modifier = Modifier.fillMaxWidth(),
        shape = MorphoTheme.radii.shapeLg,
        colors = CardDefaults.elevatedCardColors(
            containerColor = MaterialTheme.colorScheme.surfaceContainerLow,
        ),
        elevation = CardDefaults.elevatedCardElevation(defaultElevation = MorphoTheme.elevations.raised),
    ) {
        Column(
            modifier = Modifier.padding(spacing.xl),
            verticalArrangement = Arrangement.spacedBy(spacing.md),
        ) {
            Column(verticalArrangement = Arrangement.spacedBy(spacing.xxs)) {
                Text(
                    text = stringResource(R.string.today_title),
                    style = MaterialTheme.typography.headlineMedium,
                    color = MaterialTheme.colorScheme.onSurface,
                )
                Text(
                    text = todayLine(state),
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
            PrimaryButton(
                text = stringResource(
                    when (state.entry) {
                        TodayEntry.START -> R.string.action_start
                        TodayEntry.CONTINUE -> R.string.action_continue
                        TodayEntry.SUMMARY -> R.string.today_summary
                    },
                ),
                onClick = onOpenStream,
                enabled = !state.loading,
            )
        }
    }
}

@Composable
private fun todayLine(state: TodayUiState): String {
    if (state.dueReviews == 0 && state.newWords == 0) return stringResource(R.string.today_all_done)
    val line = stringResource(
        R.string.today_line,
        pluralStringResource(R.plurals.today_reviews, state.dueReviews, state.dueReviews),
        pluralStringResource(R.plurals.today_new_words, state.newWords, state.newWords),
    )
    return if (state.backlogged) line + stringResource(R.string.today_backlog) else line
}

/** Units as progress only: how many of each unit's words are in review. */
@Composable
private fun UnitsSection(state: TodayUiState) {
    val spacing = MorphoTheme.spacing
    Column(verticalArrangement = Arrangement.spacedBy(spacing.sm)) {
        SectionHeading(
            text = stringResource(R.string.today_section_units),
            trailing = {
                Text(
                    text = stringResource(R.string.today_units_finished, state.unitsFinished, state.unitCount),
                    style = MaterialTheme.typography.labelMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            },
        )
        state.units.chunked(2).forEach { row ->
            Row(horizontalArrangement = Arrangement.spacedBy(spacing.xs)) {
                row.forEach { unit -> UnitTile(unit, Modifier.weight(1f)) }
                if (row.size == 1) Spacer(Modifier.weight(1f))
            }
        }
    }
}

@Composable
private fun UnitTile(unit: UnitProgress, modifier: Modifier = Modifier) {
    val spacing = MorphoTheme.spacing
    val description = stringResource(R.string.cd_unit_progress, unit.number, unit.graduated, unit.size)
    Column(
        modifier = modifier
            .clip(MorphoTheme.radii.shapeMd)
            .background(MaterialTheme.colorScheme.surfaceContainerLow)
            .heightIn(min = spacing.minTouchTarget)
            .padding(spacing.sm)
            .semantics(mergeDescendants = true) { contentDescription = description },
        verticalArrangement = Arrangement.spacedBy(spacing.xs),
    ) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.Bottom,
        ) {
            Text(
                text = stringResource(R.string.today_unit, unit.number),
                style = MaterialTheme.typography.titleMedium,
                color = MaterialTheme.colorScheme.onSurface,
            )
            Text(
                text = if (unit.finished) {
                    stringResource(R.string.today_unit_done)
                } else {
                    stringResource(R.string.today_unit_tally, unit.graduated, unit.size)
                },
                style = MaterialTheme.typography.labelMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        ProgressTrack(fraction = unit.graduated.toFloat() / unit.size.coerceAtLeast(1))
    }
}

/** A 4dp rounded bar: the line colour as its track, ink as its fill. */
@Composable
internal fun ProgressTrack(fraction: Float, modifier: Modifier = Modifier) {
    Box(
        modifier
            .fillMaxWidth()
            .height(MorphoTheme.sizes.trackHeight)
            .clip(MorphoTheme.radii.shapeFull)
            .background(MorphoTheme.accents.ringTrack),
    ) {
        Box(
            Modifier
                .fillMaxWidth(fraction.coerceIn(0f, 1f))
                .height(MorphoTheme.sizes.trackHeight)
                .clip(MorphoTheme.radii.shapeFull)
                .background(MaterialTheme.colorScheme.primary),
        )
    }
}

/** 5500 -> "5,500". Kept local: it is presentation, not domain. */
internal fun formatCount(value: Int): String =
    java.text.NumberFormat.getIntegerInstance().format(value)

@ScreenPreviews
@Composable
private fun TodayContentPreview() {
    val today = LocalDate.now()
    ScreenPreviewBox {
        TodayContent(
            state = TodayUiState(
                loading = false,
                greetingPeriod = GreetingPeriod.AFTERNOON,
                streakDays = 12,
                dueReviews = 14,
                newWords = 20,
                entry = TodayEntry.CONTINUE,
                journey = OverallProgress(totalWords = 3909, learnedWords = 212, inFlightWords = 3),
                pace = JourneyPace(unit = 11, unitCount = 196, newPerDay = 20),
                weeklyActivity = (6 downTo 0).map { daysAgo ->
                    DailyActivity(today.minusDays(daysAgo.toLong()), listOf(12, 45, 30, 0, 55, 20, 38)[6 - daysAgo])
                },
                units = listOf(
                    UnitProgress(10, 20, 20),
                    UnitProgress(11, 20, 9),
                    UnitProgress(12, 20, 0),
                    UnitProgress(13, 20, 0),
                ),
                unitsFinished = 10,
                unitCount = 196,
            ),
            onOpenStream = {},
            onOpenSettings = {},
            onToggleChartStyle = {},
            onOpenWord = {},
        )
    }
}
