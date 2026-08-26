package dev.morpho.ui.home

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import dev.morpho.di.AppContainer
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

            val shipped = content.shippedWordIds()
            val rows = progress.allProgress()
            val today = LocalDate.now()

            _state.value = HomeUiState(
                loading = false,
                overall = ProgressTracker.overall(shipped, rows),
                today = ProgressTracker.today(
                    stats = progress.statsFor(today),
                    dailyGoal = container.settingsRepository.settings.value.dailyGoal,
                    dueReviewCount = progress.dueCount(Instant.now()),
                ),
                streakDays = ProgressTracker.streak(progress.recentStats(), today),
                contentVersion = content.contentVersion(),
            )
        }
    }

    companion object {
        fun factory(container: AppContainer) = viewModelFactory {
            initializer { HomeViewModel(container) }
        }
    }
}
