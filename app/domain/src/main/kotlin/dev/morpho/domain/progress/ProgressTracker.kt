package dev.morpho.domain.progress

import dev.morpho.domain.model.DailyActivity
import dev.morpho.domain.model.DailyStats
import dev.morpho.domain.model.GreetingPeriod
import dev.morpho.domain.model.HeatmapCell
import dev.morpho.domain.model.HeatmapData
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
     * Consecutive-day streak ending today (or yesterday — a day is not broken until
     * it is over). [history] may be in any order; only dates with activity count: a word
     * met or reviewed, or any graded step answered.
     */
    fun streak(history: Collection<DailyStats>, today: LocalDate): Int {
        val active = history
            .filter { it.newLearned > 0 || it.reviewed > 0 || it.answerCount > 0 }
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

    fun greetingPeriod(hour: Int): GreetingPeriod = when (hour) {
        in 5..11 -> GreetingPeriod.MORNING
        in 12..17 -> GreetingPeriod.AFTERNOON
        else -> GreetingPeriod.EVENING
    }

    /** The last seven days, oldest first, with how many graded steps each one answered. */
    fun weeklyActivity(
        recentStats: Collection<DailyStats>,
        today: LocalDate,
    ): List<DailyActivity> {
        val lookup = recentStats.associateBy { it.date }
        return (6 downTo 0).map { daysAgo ->
            val date = today.minusDays(daysAgo.toLong())
            DailyActivity(date = date, answers = lookup[date]?.answerCount ?: 0)
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
            activities.add(lookup[cursor]?.answerCount ?: 0)
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

    /**
     * Folds counts into the day's row. Every field is a count, so merging is pure
     * addition: a day folded step by step lands on the same numbers as one folded at once.
     */
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

    /** 0f..1f for the journey rail. */
    val fraction: Float
        get() = if (totalWords == 0) 0f else learnedWords.toFloat() / totalWords
}
