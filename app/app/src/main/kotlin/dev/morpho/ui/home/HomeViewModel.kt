package dev.morpho.ui.home

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import dev.morpho.di.AppContainer
import dev.morpho.domain.model.ActivityChartStyle
import dev.morpho.domain.model.DailyActivity
import dev.morpho.domain.model.GreetingPeriod
import dev.morpho.domain.model.HeatmapData
import dev.morpho.domain.model.ProgressDefaults
import dev.morpho.domain.progress.OverallProgress
import dev.morpho.domain.progress.ProgressTracker
import dev.morpho.domain.progress.TodayProgress
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import java.time.Instant
import java.time.LocalDate
import java.time.LocalTime

data class HomeUiState(
    val loading: Boolean = true,
    val overall: OverallProgress = OverallProgress(0, 0, 0),
    val today: TodayProgress = TodayProgress(
        newLearned = 0,
        dailyGoal = ProgressDefaults.DAILY_GOAL,
        reviewed = 0,
        dueReviews = 0,
    ),
    val streakDays: Int = 0,
    val contentVersion: String? = null,
    val greetingPeriod: GreetingPeriod = GreetingPeriod.MORNING,
    val weeklyActivity: List<DailyActivity> = emptyList(),
    val heatmapData: HeatmapData = HeatmapData(emptyList(), 0, 0),
    val estimatedDaysRemaining: Int? = null,
    val activityChartStyle: ActivityChartStyle = ActivityChartStyle.BAR,
) {
    val hasContent: Boolean get() = overall.totalWords > 0
}

class HomeViewModel(private val container: AppContainer) : ViewModel() {

    private val _state = MutableStateFlow(HomeUiState())
    val state: StateFlow<HomeUiState> = _state.asStateFlow()

    fun refresh() {
        viewModelScope.launch {
            val content = container.contentRepository
            val progress = container.progressRepository
            val settings = container.settingsRepository.settings.value

            val shipped = content.shippedWordIds()
            val rows = progress.allProgress()
            val today = LocalDate.now()
            val recentStats = progress.recentStats(120)
            val overall = ProgressTracker.overall(shipped, rows)

            _state.value = HomeUiState(
                loading = false,
                overall = overall,
                today = ProgressTracker.today(
                    stats = progress.statsFor(today),
                    dailyGoal = settings.dailyGoal,
                    dueReviewCount = progress.dueCount(Instant.now()),
                ),
                streakDays = ProgressTracker.streak(recentStats, today),
                contentVersion = content.contentVersion(),
                greetingPeriod = ProgressTracker.greetingPeriod(LocalTime.now().hour),
                weeklyActivity = ProgressTracker.weeklyActivity(recentStats, today),
                heatmapData = ProgressTracker.heatmapData(recentStats, today),
                estimatedDaysRemaining = ProgressTracker.estimatedDaysRemaining(
                    overall.remainingWords, recentStats,
                ),
                activityChartStyle = settings.activityChartStyle,
            )
        }
    }

    fun setActivityChartStyle(style: ActivityChartStyle) {
        _state.value = _state.value.copy(activityChartStyle = style)
        viewModelScope.launch {
            container.settingsRepository.setActivityChartStyle(style)
        }
    }

    companion object {
        fun factory(container: AppContainer) = viewModelFactory {
            initializer { HomeViewModel(container) }
        }
    }
}
