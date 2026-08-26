package dev.morpho.domain.progress

import dev.morpho.domain.model.DailyActivity
import dev.morpho.domain.model.DailyStats
import dev.morpho.domain.model.GreetingPeriod
import dev.morpho.domain.model.HeatmapCell
import dev.morpho.domain.model.HeatmapData
import dev.morpho.domain.model.LearningProgress
import dev.morpho.domain.model.LearningStatus
import java.time.DayOfWeek
import java.time.LocalDate

/**
 * Derives every counter the UI shows.
 *
 * Numerators come from user.db, **denominators from release.db** (README Part 6):
 * when a content update ships more words the progress bar re-scales itself with zero
 * migration, and progress rows for words a release dropped simply fail the join and
 * stop counting.
 */
object ProgressTracker {

    /**
     * @param shippedWordIds every word in the current release — the denominator
     * @param progress all progress rows, including orphans from older releases
     */
    fun overall(
        shippedWordIds: Set<Long>,
        progress: Collection<LearningProgress>,
    ): OverallProgress {
        var learned = 0
        var inFlight = 0
        for (row in progress) {
            if (row.wordId !in shippedWordIds) continue // orphan from an older release
            when (row.status) {
                LearningStatus.LEARNED -> learned++
                LearningStatus.LEARNING -> if (row.roundsPassed > 0) inFlight++
            }
        }
        return OverallProgress(
            totalWords = shippedWordIds.size,
            learnedWords = learned,
            inFlightWords = inFlight,
        )
    }

    fun today(
        stats: DailyStats?,
        dailyGoal: Int,
        dueReviewCount: Int,
    ): TodayProgress = TodayProgress(
        newLearned = stats?.newLearned ?: 0,
        dailyGoal = dailyGoal.coerceAtLeast(1),
        reviewed = stats?.reviewed ?: 0,
        dueReviews = dueReviewCount,
        correctCount = stats?.correctCount ?: 0,
        answerCount = stats?.answerCount ?: 0,
    )

    /**
     * Consecutive-day streak ending today (or yesterday — a day is not broken until
     * it is over). [history] may be in any order; only dates with activity count.
     */
    fun streak(history: Collection<DailyStats>, today: LocalDate): Int {
        val active = history
            .filter { it.newLearned > 0 || it.reviewed > 0 }
            .map { it.date }
            .toSortedSet()
        if (active.isEmpty()) return 0

        var cursor = when {
            active.contains(today) -> today
            active.contains(today.minusDays(1)) -> today.minusDays(1)
            else -> return 0
        }
        var count = 0
        while (active.contains(cursor)) {
            count++
            cursor = cursor.minusDays(1)
        }
        return count
    }

    /**
     * Folds one session's results into the day's row.
     *
     * Every field is a count, so merging is pure addition: the third session of a day
     * lands on exactly the same numbers whether the day is folded session-by-session or
     * all at once. (The wave-1 row stored a rate and had to reconstruct prior counts to
     * re-average, which lost precision and mis-weighted sessions of unequal length.)
     */
    fun greetingPeriod(hour: Int): GreetingPeriod = when (hour) {
        in 5..11 -> GreetingPeriod.MORNING
        in 12..17 -> GreetingPeriod.AFTERNOON
        else -> GreetingPeriod.EVENING
    }

    fun weeklyActivity(
        recentStats: Collection<DailyStats>,
        today: LocalDate,
    ): List<DailyActivity> {
        val lookup = recentStats.associateBy { it.date }
        return (6 downTo 0).map { daysAgo ->
            val date = today.minusDays(daysAgo.toLong())
            val stats = lookup[date]
            DailyActivity(
                date = date,
                wordsStudied = (stats?.newLearned ?: 0) + (stats?.reviewed ?: 0),
                newLearned = stats?.newLearned ?: 0,
                reviewed = stats?.reviewed ?: 0,
            )
        }
    }

    fun heatmapData(
        recentStats: Collection<DailyStats>,
        today: LocalDate,
        weeks: Int = 16,
    ): HeatmapData {
        val lookup = recentStats.associateBy { it.date }
        val rawStart = today.minusDays((weeks * 7 - 1).toLong())
        val start = rawStart.with(DayOfWeek.MONDAY)

        val activities = mutableListOf<Int>()
        val rawCells = mutableListOf<HeatmapCell>()
        var cursor = start
        while (!cursor.isAfter(today)) {
            val stats = lookup[cursor]
            val activity = (stats?.newLearned ?: 0) + (stats?.reviewed ?: 0)
            activities.add(activity)
            rawCells.add(HeatmapCell(date = cursor, intensity = 0))
            cursor = cursor.plusDays(1)
        }

        val maxAct = activities.maxOrNull() ?: 0
        val cells = rawCells.mapIndexed { i, cell ->
            val act = activities[i]
            val intensity = when {
                act == 0 -> 0
                maxAct == 0 -> 0
                else -> {
                    val ratio = act.toFloat() / maxAct
                    when {
                        ratio <= 0.25f -> 1
                        ratio <= 0.50f -> 2
                        ratio <= 0.75f -> 3
                        else -> 4
                    }
                }
            }
            cell.copy(intensity = intensity)
        }
        return HeatmapData(
            cells = cells,
            weeks = (cells.size + 6) / 7,
            maxActivity = maxAct,
        )
    }

    fun estimatedDaysRemaining(
        remainingWords: Int,
        recentStats: Collection<DailyStats>,
    ): Int? {
        if (remainingWords <= 0) return 0
        val activeDays = recentStats.filter { it.newLearned > 0 }
        if (activeDays.isEmpty()) return null
        val recent = activeDays.take(14)
        val avgPerDay = recent.sumOf { it.newLearned }.toFloat() / recent.size
        if (avgPerDay < 0.1f) return null
        return (remainingWords / avgPerDay).toInt().coerceAtLeast(1)
    }

    fun mergeSession(
        existing: DailyStats?,
        date: LocalDate,
        newLearned: Int,
        reviewed: Int,
        correctAnswers: Int,
        totalAnswers: Int,
    ): DailyStats = DailyStats(
        date = date,
        newLearned = (existing?.newLearned ?: 0) + newLearned,
        reviewed = (existing?.reviewed ?: 0) + reviewed,
        correctCount = (existing?.correctCount ?: 0) + correctAnswers,
        answerCount = (existing?.answerCount ?: 0) + totalAnswers,
    )
}

data class OverallProgress(
    val totalWords: Int,
    val learnedWords: Int,
    val inFlightWords: Int,
) {
    val remainingWords: Int get() = (totalWords - learnedWords).coerceAtLeast(0)

    /** 0f..1f for the home ProgressRing. */
    val fraction: Float
        get() = if (totalWords == 0) 0f else learnedWords.toFloat() / totalWords
}

data class TodayProgress(
    val newLearned: Int,
    val dailyGoal: Int,
    val reviewed: Int,
    val dueReviews: Int,
    val correctCount: Int = 0,
    val answerCount: Int = 0,
) {
    /** Derived from the stored counts, never persisted. */
    val correctRate: Double?
        get() = if (answerCount == 0) null else correctCount.toDouble() / answerCount

    val remainingNew: Int get() = (dailyGoal - newLearned).coerceAtLeast(0)
    val goalMet: Boolean get() = newLearned >= dailyGoal
    val fraction: Float get() = (newLearned.toFloat() / dailyGoal).coerceIn(0f, 1f)
    val hasWork: Boolean get() = dueReviews > 0 || !goalMet
}
