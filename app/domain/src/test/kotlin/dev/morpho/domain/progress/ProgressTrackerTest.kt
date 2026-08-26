package dev.morpho.domain.progress

import dev.morpho.domain.model.DailyStats
import dev.morpho.domain.model.LearningProgress
import dev.morpho.domain.model.LearningStatus
import java.time.LocalDate
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull
import kotlin.test.assertTrue

class ProgressTrackerTest {

    private val today = LocalDate.of(2026, 5, 20)

    @Test
    fun `denominator comes from the release, numerator from user progress`() {
        val shipped = (1L..100L).toSet()
        val progress = listOf(
            LearningProgress(1, roundsPassed = 3, status = LearningStatus.LEARNED),
            LearningProgress(2, roundsPassed = 3, status = LearningStatus.LEARNED),
            LearningProgress(3, roundsPassed = 1, status = LearningStatus.LEARNING),
            LearningProgress(4, roundsPassed = 0, status = LearningStatus.LEARNING),
        )
        val overall = ProgressTracker.overall(shipped, progress)
        assertEquals(100, overall.totalWords)
        assertEquals(2, overall.learnedWords)
        assertEquals(1, overall.inFlightWords)
        assertEquals(98, overall.remainingWords)
        assertEquals(0.02f, overall.fraction)
    }

    @Test
    fun `progress rows for words the release dropped stop counting but are not an error`() {
        val shipped = setOf(1L, 2L)
        val progress = listOf(
            LearningProgress(1, status = LearningStatus.LEARNED),
            LearningProgress(999, status = LearningStatus.LEARNED), // retired word
        )
        val overall = ProgressTracker.overall(shipped, progress)
        assertEquals(2, overall.totalWords)
        assertEquals(1, overall.learnedWords)
    }

    @Test
    fun `a bigger release rescales the ring without touching progress`() {
        val progress = listOf(LearningProgress(1, status = LearningStatus.LEARNED))
        val before = ProgressTracker.overall((1L..10L).toSet(), progress)
        val after = ProgressTracker.overall((1L..20L).toSet(), progress)
        assertEquals(0.1f, before.fraction)
        assertEquals(0.05f, after.fraction)
        assertEquals(before.learnedWords, after.learnedWords)
    }

    @Test
    fun `today's card combines daily stats, goal and due reviews`() {
        val stats = DailyStats(
            today,
            newLearned = 20,
            reviewed = 8,
            correctCount = 21,
            answerCount = 28,
        )
        val progress = ProgressTracker.today(stats, dailyGoal = 50, dueReviewCount = 12)
        assertEquals(30, progress.remainingNew)
        assertEquals(0.4f, progress.fraction)
        assertEquals(0.75, progress.correctRate!!, 1e-9)
        assertTrue(progress.hasWork)
        assertTrue(!progress.goalMet)

        val done = ProgressTracker.today(
            DailyStats(today, newLearned = 50, reviewed = 12),
            dailyGoal = 50,
            dueReviewCount = 0,
        )
        assertTrue(done.goalMet)
        assertTrue(!done.hasWork)
        assertEquals(1f, done.fraction)
    }

    @Test
    fun `a missing daily row reads as a fresh day`() {
        val progress = ProgressTracker.today(null, dailyGoal = 30, dueReviewCount = 0)
        assertEquals(0, progress.newLearned)
        assertEquals(30, progress.remainingNew)
        assertNull(progress.correctRate)
    }

    @Test
    fun `streak counts consecutive active days ending today`() {
        val history = listOf(
            DailyStats(today, newLearned = 5),
            DailyStats(today.minusDays(1), reviewed = 3),
            DailyStats(today.minusDays(2), newLearned = 10),
            DailyStats(today.minusDays(4), newLearned = 10),
        )
        assertEquals(3, ProgressTracker.streak(history, today))
    }

    @Test
    fun `a day with no activity does not extend the streak`() {
        val history = listOf(
            DailyStats(today, newLearned = 0, reviewed = 0),
            DailyStats(today.minusDays(1), newLearned = 4),
            DailyStats(today.minusDays(2), newLearned = 4),
        )
        // Today is still empty, so the streak is measured from yesterday and survives.
        assertEquals(2, ProgressTracker.streak(history, today))
    }

    @Test
    fun `a two day gap resets the streak`() {
        val history = listOf(
            DailyStats(today.minusDays(2), newLearned = 4),
            DailyStats(today.minusDays(3), newLearned = 4),
        )
        assertEquals(0, ProgressTracker.streak(history, today))
    }

    @Test
    fun `merging a session accumulates every count`() {
        val first = ProgressTracker.mergeSession(
            existing = null,
            date = today,
            newLearned = 10,
            reviewed = 0,
            correctAnswers = 8,
            totalAnswers = 10,
        )
        assertEquals(10, first.newLearned)
        assertEquals(8, first.correctCount)
        assertEquals(10, first.answerCount)
        assertEquals(0.8, first.correctRate!!, 1e-9)

        val second = ProgressTracker.mergeSession(
            existing = first,
            date = today,
            newLearned = 0,
            reviewed = 10,
            correctAnswers = 10,
            totalAnswers = 10,
        )
        assertEquals(10, second.newLearned)
        assertEquals(10, second.reviewed)
        assertEquals(18, second.correctCount)
        assertEquals(20, second.answerCount)
        assertEquals(0.9, second.correctRate!!, 1e-9)
    }

    @Test
    fun `a day answered in many sessions equals the same day answered in one`() {
        // The property the count columns exist for. Sessions of wildly unequal length
        // are the case a stored rate got wrong: re-averaging weighted a 3-answer session
        // as heavily as a 97-answer one.
        val sessions = listOf(
            Triple(3, 2, 3),      // newLearned, correct, answered
            Triple(0, 91, 97),
            Triple(7, 4, 11),
            Triple(0, 0, 4),
        )

        var folded: DailyStats? = null
        sessions.forEach { (learned, correct, answered) ->
            folded = ProgressTracker.mergeSession(
                existing = folded,
                date = today,
                newLearned = learned,
                reviewed = if (learned == 0) answered else 0,
                correctAnswers = correct,
                totalAnswers = answered,
            )
        }

        val atOnce = ProgressTracker.mergeSession(
            existing = null,
            date = today,
            newLearned = sessions.sumOf { it.first },
            reviewed = sessions.filter { it.first == 0 }.sumOf { it.third },
            correctAnswers = sessions.sumOf { it.second },
            totalAnswers = sessions.sumOf { it.third },
        )

        assertEquals(atOnce, folded)
        assertEquals(97, folded!!.correctCount)
        assertEquals(115, folded!!.answerCount)
    }

    @Test
    fun `merge order does not change the day`() {
        val a = Triple(5, 4, 5)
        val b = Triple(0, 17, 40)

        fun fold(first: Triple<Int, Int, Int>, second: Triple<Int, Int, Int>): DailyStats {
            val one = ProgressTracker.mergeSession(
                null, today, first.first, 0, first.second, first.third,
            )
            return ProgressTracker.mergeSession(
                one, today, second.first, 0, second.second, second.third,
            )
        }

        assertEquals(fold(a, b), fold(b, a))
    }

    @Test
    fun `a day with no answers has no accuracy at all`() {
        val merged = ProgressTracker.mergeSession(
            existing = null,
            date = today,
            newLearned = 0,
            reviewed = 0,
            correctAnswers = 0,
            totalAnswers = 0,
        )
        assertEquals(0, merged.answerCount)
        assertNull(merged.correctRate)
    }

    @Test
    fun `a perfect day reads as exactly one, not a rounded rate`() {
        var day: DailyStats? = null
        repeat(7) {
            day = ProgressTracker.mergeSession(day, today, 3, 0, 3, 3)
        }
        assertEquals(21, day!!.correctCount)
        assertEquals(21, day!!.answerCount)
        assertEquals(1.0, day!!.correctRate!!, 0.0)
    }
}
