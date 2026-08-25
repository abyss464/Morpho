package dev.morpho.domain.review

import dev.morpho.domain.model.CardState
import dev.morpho.domain.model.FsrsCard
import java.time.Duration
import java.time.Instant
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

/**
 * Vectors were computed independently from the published FSRS-5 formulas with the
 * default weights and desired retention 0.9, then pinned here. Any change to the
 * implementation that moves these numbers is a behavioural change, not a refactor.
 */
class FsrsSchedulerTest {

    private val scheduler = FsrsScheduler()
    private val t0: Instant = Instant.parse("2026-01-01T08:00:00Z")

    private fun assertClose(expected: Double, actual: Double, eps: Double = 1e-9) {
        assertTrue(
            kotlin.math.abs(expected - actual) <= eps,
            "expected $expected but was $actual (eps $eps)",
        )
    }

    @Test
    fun `factor and decay match the published constants`() {
        assertEquals(-0.5, Fsrs.DECAY)
        // FACTOR = 0.9^(1/DECAY) - 1 = 19/81
        assertClose(19.0 / 81.0, Fsrs.FACTOR, 1e-12)
    }

    @Test
    fun `initial stability is the weight for the grade`() {
        assertClose(0.40255, scheduler.initialStability(Grade.AGAIN))
        assertClose(1.18385, scheduler.initialStability(Grade.HARD))
        assertClose(3.173, scheduler.initialStability(Grade.GOOD))
        assertClose(15.69105, scheduler.initialStability(Grade.EASY))
    }

    @Test
    fun `initial difficulty follows the exponential form`() {
        // D0(G) = w4 - e^(w5*(G-1)) + 1
        assertClose(7.1949, scheduler.initialDifficulty(Grade.AGAIN), 1e-12)
        assertClose(5.282434422319005, scheduler.initialDifficulty(Grade.GOOD), 1e-12)
        assertClose(3.2245015893713678, scheduler.initialDifficulty(Grade.EASY), 1e-12)
    }

    @Test
    fun `interval at desired retention 0_9 equals stability`() {
        // I(0.9, S) = (S/FACTOR) * (0.9^-2 - 1) = S, by construction of FACTOR.
        assertEquals(100L, scheduler.intervalDays(100.0))
        assertEquals(3L, scheduler.intervalDays(3.173))
        assertEquals(1L, scheduler.intervalDays(0.05)) // floored at one day
    }

    @Test
    fun `retrievability follows the power forgetting curve`() {
        assertClose(1.0, scheduler.retrievability(0.0, 3.173), 1e-12)
        assertClose(0.7582588166545682, scheduler.retrievability(10.0, 3.173), 1e-12)
        // By definition R(S, S) == 0.9
        assertClose(0.9, scheduler.retrievability(3.173, 3.173), 1e-12)
    }

    @Test
    fun `vector 1 - first review graded Good`() {
        val card = scheduler.newCard(wordId = 1, now = t0, grade = Grade.GOOD)
        assertClose(3.173, card.stability, 1e-12)
        assertClose(5.282434422319005, card.difficulty, 1e-12)
        assertEquals(3, card.scheduledDays)
        assertEquals(1, card.reps)
        assertEquals(0, card.lapses)
        assertEquals(CardState.REVIEW, card.state)
        assertEquals(t0.plus(Duration.ofDays(3)), card.due)
    }

    @Test
    fun `vector 2 - successful recall three days later`() {
        val first = scheduler.newCard(wordId = 1, now = t0, grade = Grade.GOOD)
        val t1 = t0.plus(Duration.ofDays(3))
        assertClose(0.9046982108893272, scheduler.retrievability(first, t1), 1e-12)

        val second = scheduler.review(first, Grade.GOOD, t1)
        assertClose(10.73892584613159, second.stability, 1e-9)
        assertClose(5.272967931287446, second.difficulty, 1e-12)
        assertEquals(11, second.scheduledDays)
        assertEquals(3, second.elapsedDays)
        assertEquals(2, second.reps)
        assertEquals(0, second.lapses)
        assertEquals(CardState.REVIEW, second.state)
    }

    @Test
    fun `vector 3 - lapse collapses stability and bumps difficulty`() {
        val first = scheduler.newCard(wordId = 1, now = t0, grade = Grade.GOOD)
        val second = scheduler.review(first, Grade.GOOD, t0.plus(Duration.ofDays(3)))
        val t2 = t0.plus(Duration.ofDays(3 + 11))

        val third = scheduler.review(second, Grade.AGAIN, t2)
        assertClose(2.185775232039225, third.stability, 1e-9)
        assertClose(6.790567694566929, third.difficulty, 1e-12)
        assertEquals(2, third.scheduledDays)
        assertEquals(1, third.lapses)
        assertEquals(CardState.RELEARNING, third.state)
    }

    @Test
    fun `vector 4 - same day repeat uses short term stability`() {
        val first = scheduler.newCard(wordId = 1, now = t0, grade = Grade.GOOD)
        val sameDay = scheduler.review(first, Grade.GOOD, t0.plus(Duration.ofHours(2)))
        // S * e^(w17 * (G - 3 + w18)) with G = 3
        assertClose(4.466858064362218, sameDay.stability, 1e-9)
        assertEquals(0, sameDay.elapsedDays)
        assertEquals(CardState.REVIEW, sameDay.state)
    }

    @Test
    fun `a lapse never increases stability`() {
        var card = scheduler.newCard(wordId = 7, now = t0, grade = Grade.EASY)
        repeat(4) {
            card = scheduler.review(card, Grade.GOOD, card.due)
        }
        val before = card.stability
        val lapsed = scheduler.review(card, Grade.AGAIN, card.due)
        assertTrue(lapsed.stability < before, "lapse raised stability: $before -> ${lapsed.stability}")
    }

    @Test
    fun `difficulty stays clamped across long histories`() {
        var card = scheduler.newCard(wordId = 9, now = t0, grade = Grade.AGAIN)
        repeat(30) {
            card = scheduler.review(card, Grade.AGAIN, card.due)
            assertTrue(card.difficulty in 1.0..10.0, "difficulty escaped: ${card.difficulty}")
            assertTrue(card.stability >= Fsrs.MIN_STABILITY)
        }
        repeat(30) {
            card = scheduler.review(card, Grade.EASY, card.due)
            assertTrue(card.difficulty in 1.0..10.0, "difficulty escaped: ${card.difficulty}")
        }
    }

    @Test
    fun `intervals are capped at the maximum`() {
        val huge = FsrsCard(
            wordId = 1,
            due = t0,
            stability = 1_000_000.0,
            difficulty = 5.0,
            state = CardState.REVIEW,
            lastReview = t0,
        )
        val next = scheduler.review(huge, Grade.EASY, t0.plus(Duration.ofDays(1)))
        assertEquals(Fsrs.MAX_INTERVAL_DAYS.toInt(), next.scheduledDays)
    }
}
