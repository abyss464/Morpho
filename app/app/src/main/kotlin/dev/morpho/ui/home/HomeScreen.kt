package dev.morpho.ui.home

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.BarChart
import androidx.compose.material.icons.rounded.GridView
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import dev.morpho.R
import dev.morpho.data.sound.SfxEvent
import dev.morpho.di.AppContainer
import dev.morpho.di.StartupReport
import dev.morpho.domain.model.ActivityChartStyle
import dev.morpho.domain.model.DailyActivity
import dev.morpho.domain.model.GreetingPeriod
import dev.morpho.domain.model.HeatmapCell
import dev.morpho.domain.model.HeatmapData
import dev.morpho.domain.progress.OverallProgress
import dev.morpho.domain.progress.TodayProgress
import dev.morpho.ui.designsystem.component.MotifOrnament
import dev.morpho.ui.designsystem.component.PreviewBox
import dev.morpho.ui.designsystem.component.SectionHeading
import dev.morpho.ui.designsystem.component.StatTile
import dev.morpho.ui.designsystem.component.ThemePreviews
import dev.morpho.ui.designsystem.theme.MorphoTheme
import java.time.LocalDate

@Composable
fun HomeScreen(
    container: AppContainer,
    startup: StartupReport,
    onStartLearning: () -> Unit,
    onStartReview: () -> Unit,
    onOpenSettings: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val viewModel: HomeViewModel = viewModel(factory = HomeViewModel.factory(container))
    val state by viewModel.state.collectAsStateWithLifecycle()

    LaunchedEffect(Unit) { viewModel.refresh() }

    Scaffold(
        modifier = modifier.fillMaxSize(),
    ) { padding ->
        HomeContent(
            state = state,
            modifier = Modifier
                .fillMaxSize()
                .padding(padding),
            onStartLearning = {
                container.playSfx(SfxEvent.TAP)
                onStartLearning()
            },
            onStartReview = {
                container.playSfx(SfxEvent.TAP)
                onStartReview()
            },
            onOpenSettings = {
                container.playSfx(SfxEvent.TAP)
                onOpenSettings()
            },
            onToggleChartStyle = { style ->
                container.playSfx(SfxEvent.TAP)
                viewModel.setActivityChartStyle(style)
            },
        )
    }
}

@Composable
private fun HomeContent(
    state: HomeUiState,
    onStartLearning: () -> Unit,
    onStartReview: () -> Unit,
    onOpenSettings: () -> Unit,
    onToggleChartStyle: (ActivityChartStyle) -> Unit,
    modifier: Modifier = Modifier,
) {
    val spacing = MorphoTheme.spacing
    Column(
        modifier = modifier
            .verticalScroll(rememberScrollState())
            .padding(horizontal = spacing.screenGutter)
            .padding(bottom = spacing.xxl),
        verticalArrangement = Arrangement.spacedBy(spacing.xl),
    ) {
        Spacer(Modifier.height(spacing.xs))

        GreetingHeader(
            greetingPeriod = state.greetingPeriod,
            streakDays = state.streakDays,
            onOpenSettings = onOpenSettings,
        )

        ActionCard(
            today = state.today,
            hasContent = state.hasContent,
            onStartLearning = onStartLearning,
            onStartReview = onStartReview,
        )

        Column(verticalArrangement = Arrangement.spacedBy(spacing.md)) {
            SectionHeading(
                text = stringResource(R.string.home_section_this_week),
                trailing = {
                    IconButton(
                        onClick = {
                            val next = when (state.activityChartStyle) {
                                ActivityChartStyle.BAR -> ActivityChartStyle.HEATMAP
                                ActivityChartStyle.HEATMAP -> ActivityChartStyle.BAR
                            }
                            onToggleChartStyle(next)
                        },
                        modifier = Modifier.size(spacing.minTouchTarget),
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

            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(spacing.sm),
            ) {
                val wordsToday = state.today.newLearned + state.today.reviewed
                val accuracy = state.today.correctRate
                StatTile(
                    value = formatCount(wordsToday),
                    label = stringResource(R.string.home_stat_words_today),
                    modifier = Modifier.weight(1f),
                )
                StatTile(
                    value = if (accuracy != null) "${(accuracy * 100).toInt()}%" else "—",
                    label = stringResource(R.string.home_stat_accuracy),
                    modifier = Modifier.weight(1f),
                )
                StatTile(
                    value = formatCount(state.overall.inFlightWords),
                    label = stringResource(R.string.home_stat_in_progress),
                    modifier = Modifier.weight(1f),
                )
            }
        }

        JourneyProgress(
            overall = state.overall,
            estimatedDaysRemaining = state.estimatedDaysRemaining,
        )

        MotifOrnament()
    }
}

/** 5500 -> "5,500". Kept local: it is presentation, not domain. */
internal fun formatCount(value: Int): String =
    java.text.NumberFormat.getIntegerInstance().format(value)

@ThemePreviews
@Composable
private fun HomeContentPreview() {
    val today = LocalDate.now()
    PreviewBox {
        Box(Modifier.fillMaxWidth()) {
            HomeContent(
                state = HomeUiState(
                    loading = false,
                    overall = OverallProgress(totalWords = 4253, learnedWords = 2090, inFlightWords = 17),
                    today = TodayProgress(
                        newLearned = 20,
                        dailyGoal = 50,
                        reviewed = 8,
                        dueReviews = 12,
                        correctCount = 23,
                        answerCount = 25,
                    ),
                    streakDays = 12,
                    contentVersion = "2026.08.26+ff7fd531",
                    greetingPeriod = GreetingPeriod.EVENING,
                    weeklyActivity = (6 downTo 0).map { daysAgo ->
                        DailyActivity(
                            date = today.minusDays(daysAgo.toLong()),
                            wordsStudied = listOf(12, 45, 30, 0, 55, 20, 38)[6 - daysAgo],
                            newLearned = listOf(8, 20, 15, 0, 25, 10, 18)[6 - daysAgo],
                            reviewed = listOf(4, 25, 15, 0, 30, 10, 20)[6 - daysAgo],
                        )
                    },
                    heatmapData = HeatmapData(
                        cells = (0 until 112).map { i ->
                            HeatmapCell(
                                date = today.minusDays((111 - i).toLong()),
                                intensity = listOf(0, 1, 2, 3, 4, 0, 1, 2, 3, 0)[i % 10],
                            )
                        },
                        weeks = 16,
                        maxActivity = 50,
                    ),
                    estimatedDaysRemaining = 43,
                    activityChartStyle = ActivityChartStyle.BAR,
                ),
                onStartLearning = {},
                onStartReview = {},
                onOpenSettings = {},
                onToggleChartStyle = {},
            )
        }
    }
}
