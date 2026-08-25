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
    fun `spell probability rises with lapses and is capped`() {
        assertEquals(0.20, scheduler.spellProbability(0), 1e-12)
        assertEquals(0.40, scheduler.spellProbability(1), 1e-12)
        assertEquals(0.60, scheduler.spellProbability(2), 1e-12)
        assertEquals(0.80, scheduler.spellProbability(3), 1e-12)
        assertEquals(0.80, scheduler.spellProbability(40), 1e-12)
    }

    @Test
    fun `heavily lapsed cards skew towards listening spell`() {
        val fresh = (1L..400L).map { card(it, -1, lapses = 0) }
        val lapsed = (1L..400L).map { card(it, -1, lapses = 5) }

        val freshSpell = fresh.count {
            scheduler.questionTypeFor(it, daySeed = 42L) == ReviewQuestionType.LISTENING_SPELL
        }
        val lapsedSpell = lapsed.count {
            scheduler.questionTypeFor(it, daySeed = 42L) == ReviewQuestionType.LISTENING_SPELL
        }

        assertTrue(freshSpell < fresh.size / 2, "fresh cards spelled too often: $freshSpell/400")
        assertTrue(lapsedSpell > lapsed.size / 2, "lapsed cards spelled too rarely: $lapsedSpell/400")
        assertTrue(lapsedSpell > freshSpell)
    }

    @Test
    fun `question type is stable within a day and varies across days`() {
        val c = card(77, -1, lapses = 1)
        val a = scheduler.questionTypeFor(c, daySeed = 1000L)
        val b = scheduler.questionTypeFor(c, daySeed = 1000L)
        assertEquals(a, b)

        val across = (1L..50L).map { scheduler.questionTypeFor(c, daySeed = it) }.toSet()
        assertEquals(2, across.size, "question type never varies across days")
    }

    @Test
    fun `queue respects the limit and keeps the earliest cards`() {
        val cards = (1L..10L).map { card(it, -it) }
        val queue = scheduler.buildQueue(cards, now, daySeed = 5L, limit = 3)
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
