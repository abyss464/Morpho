package dev.morpho.domain.progress

import dev.morpho.domain.model.DailyStats
import dev.morpho.domain.model.LearningProgress
import dev.morpho.domain.model.LearningStatus
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
        correctRate = stats?.correctRate,
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

    /** Folds one session's results into the day's row. */
    fun mergeSession(
        existing: DailyStats?,
        date: LocalDate,
        newLearned: Int,
        reviewed: Int,
        correctAnswers: Int,
        totalAnswers: Int,
    ): DailyStats {
        val priorReviewed = existing?.reviewed ?: 0
        val priorNew = existing?.newLearned ?: 0
        // correct_rate is stored as a rate, so recover the prior counts to re-average.
        val priorAnswers = priorReviewed + priorNew
        val priorCorrect = ((existing?.correctRate ?: 0.0) * priorAnswers)
        val totalAnswered = priorAnswers + totalAnswers
        val rate = if (totalAnswered == 0) null else (priorCorrect + correctAnswers) / totalAnswered
        return DailyStats(
            date = date,
            newLearned = priorNew + newLearned,
            reviewed = priorReviewed + reviewed,
            correctRate = rate,
        )
    }
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
    val correctRate: Double?,
) {
    val remainingNew: Int get() = (dailyGoal - newLearned).coerceAtLeast(0)
    val goalMet: Boolean get() = newLearned >= dailyGoal
    val fraction: Float get() = (newLearned.toFloat() / dailyGoal).coerceIn(0f, 1f)
    val hasWork: Boolean get() = dueReviews > 0 || !goalMet
}
