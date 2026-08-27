package dev.morpho.domain.review

import dev.morpho.domain.model.CardState
import dev.morpho.domain.model.FsrsCard
import java.time.Duration
import java.time.Instant
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

class ReviewSchedulerTest {

    private val now: Instant = Instant.parse("2026-03-05T09:00:00Z")
    private val scheduler = ReviewScheduler()

    private fun card(id: Long, dueOffsetDays: Long, lapses: Int = 0) = FsrsCard(
        wordId = id,
        due = now.plus(Duration.ofDays(dueOffsetDays)),
        stability = 5.0,
        difficulty = 5.0,
        lapses = lapses,
        state = CardState.REVIEW,
        lastReview = now.minus(Duration.ofDays(5)),
    )

    @Test
    fun `due queue takes only cards at or past their due instant, earliest first`() {
        val cards = listOf(card(1, 2), card(2, -3), card(3, 0), card(4, -1))
        val due = scheduler.dueCards(cards, now)
        assertEquals(listOf(2L, 4L, 3L), due.map { it.wordId })
    }

    @Test
    fun `buildQueue returns ReviewItems wrapping each due card`() {
        val cards = listOf(card(1, -2), card(2, -1), card(3, 5))
        val queue = scheduler.buildQueue(cards, now)
        assertEquals(listOf(1L, 2L), queue.map { it.card.wordId })
    }

    @Test
    fun `queue respects the limit and keeps the earliest cards`() {
        val cards = (1L..10L).map { card(it, -it) }
        val queue = scheduler.buildQueue(cards, now, limit = 3)
        assertEquals(listOf(10L, 9L, 8L), queue.map { it.card.wordId })
    }

    @Test
    fun `wrong answers grade Again and count a lapse`() {
        val c = card(1, -1)
        val after = scheduler.applyAnswer(c, correct = false, now = now)
        assertEquals(1, after.lapses)
        assertEquals(CardState.RELEARNING, after.state)

        val ok = scheduler.applyAnswer(c, correct = true, now = now)
        assertEquals(0, ok.lapses)
        assertEquals(CardState.REVIEW, ok.state)
        assertTrue(ok.due.isAfter(now))
    }
}
