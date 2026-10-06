package dev.morpho.ui.today

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import dev.morpho.data.repository.IndexedWord
import dev.morpho.data.stream.StreamSnapshot
import dev.morpho.di.AppContainer
import dev.morpho.domain.model.ActivityChartStyle
import dev.morpho.domain.model.DailyActivity
import dev.morpho.domain.model.GreetingPeriod
import dev.morpho.domain.model.HeatmapData
import dev.morpho.domain.progress.OverallProgress
import dev.morpho.domain.progress.ProgressTracker
import dev.morpho.domain.stream.StreamEngine
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import java.time.Instant
import java.time.LocalDate
import java.time.LocalTime

/** What the Today card's button opens the stream as. */
enum class TodayEntry {
    /** Nothing done today yet. */
    START,

    /** Steps were taken today, or one is waiting on screen. */
    CONTINUE,

    /** Nothing is left for today; the stream opens on its done screen. */
    SUMMARY,
}

/** One unit's progress: [graduated] of its [size] words are in review. */
data class UnitProgress(val number: Int, val size: Int, val graduated: Int) {
    val finished: Boolean get() = graduated >= size
}

data class TodayUiState(
    val loading: Boolean = true,
    val greetingPeriod: GreetingPeriod = GreetingPeriod.MORNING,
    val streakDays: Int = 0,
    val dueReviews: Int = 0,
    val newWords: Int = 0,
    /** Due reviews have reached the backlog guard, so new words wait. */
    val backlogged: Boolean = false,
    val entry: TodayEntry = TodayEntry.START,
    /** Words in the release, words met (carded words plus words in the stream), words being learned. */
    val journey: OverallProgress = OverallProgress(0, 0, 0),
    val pace: JourneyPace = JourneyPace(unit = 1, unitCount = 0, newPerDay = 1),
    val weeklyActivity: List<DailyActivity> = emptyList(),
    val heatmapData: HeatmapData = HeatmapData(emptyList(), 0, 0),
    val activityChartStyle: ActivityChartStyle = ActivityChartStyle.BAR,
    /** The units around the first unfinished one. */
    val units: List<UnitProgress> = emptyList(),
    val unitsFinished: Int = 0,
    val unitCount: Int = 0,
    /** Every shipped word, for "Look up a word". */
    val index: List<IndexedWord> = emptyList(),
)

/** Today: what the stream holds for the day, and the long view around it. */
class TodayViewModel(private val container: AppContainer) : ViewModel() {

    private val _state = MutableStateFlow(TodayUiState())
    val state: StateFlow<TodayUiState> = _state.asStateFlow()

    fun refresh() {
        viewModelScope.launch {
            val engine = container.streamEngine
            val today = LocalDate.now()
            val now = Instant.now()
            val snapshot = container.streamStore.open(today)
            val stream = engine.today(snapshot.state, today)
            val settings = container.settingsRepository.settings.value
            val recentStats = container.progressRepository.recentStats(120)

            val due = engine.dueReviews(stream, snapshot.cards, snapshot.order.toSet(), now).size
            val fresh = if (due < StreamEngine.BACKLOG) engine.newAllowance(stream, settings.dailyGoal) else 0
            val shipped = snapshot.order.toSet()
            val met = snapshot.cards.keys.count { it in shipped } + stream.words.size
            val journey = OverallProgress(
                totalWords = snapshot.order.size,
                learnedWords = met,
                inFlightWords = stream.words.size,
            )
            val units = unitsOf(snapshot)
            val firstOpen = units.indexOfFirst { !it.finished }.let { if (it < 0) units.size else it }
            val from = (firstOpen - 2).coerceAtLeast(0)

            _state.value = TodayUiState(
                loading = false,
                greetingPeriod = ProgressTracker.greetingPeriod(LocalTime.now().hour),
                streakDays = ProgressTracker.streak(recentStats, today),
                dueReviews = due,
                newWords = fresh,
                backlogged = due >= StreamEngine.BACKLOG,
                entry = when {
                    due == 0 && fresh == 0 && stream.words.isEmpty() -> TodayEntry.SUMMARY
                    stream.day.steps > 0 || stream.current != null -> TodayEntry.CONTINUE
                    else -> TodayEntry.START
                },
                journey = journey,
                pace = JourneyPace(
                    unit = if (firstOpen < units.size) firstOpen + 1 else units.size,
                    unitCount = units.size,
                    newPerDay = settings.dailyGoal,
                ),
                weeklyActivity = ProgressTracker.weeklyActivity(recentStats, today),
                heatmapData = ProgressTracker.heatmapData(recentStats, today),
                activityChartStyle = settings.activityChartStyle,
                units = units.drop(from).take(UNITS_SHOWN),
                unitsFinished = units.count { it.finished },
                unitCount = units.size,
                index = container.contentRepository.wordIndex(),
            )
        }
    }

    fun setActivityChartStyle(style: ActivityChartStyle) {
        _state.value = _state.value.copy(activityChartStyle = style)
        viewModelScope.launch {
            container.settingsRepository.setActivityChartStyle(style)
        }
    }

    private fun unitsOf(snapshot: StreamSnapshot): List<UnitProgress> =
        snapshot.order.chunked(StreamSnapshot.UNIT_SIZE).mapIndexed { k, ids ->
            UnitProgress(number = k + 1, size = ids.size, graduated = ids.count { it in snapshot.cards })
        }

    companion object {
        private const val UNITS_SHOWN = 8

        fun factory(container: AppContainer) = viewModelFactory {
            initializer { TodayViewModel(container) }
        }
    }
}
