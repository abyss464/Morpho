package dev.morpho.domain.progress

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

/**
 * Backlog #35: the home counter stayed at `0 / 50` while words were plainly being
 * learned, because `daily_stats` was written only at the summary screen and a full
 * session is ~150 answers away from it. Progress is now banked as each word graduates,
 * which only works if repeating the write is harmless.
 */
class SessionBankTest {

    @Test
    fun `a word graduating mid-session is pending immediately`() {
        val banked = SessionBank(learned = 3, correctAnswers = 40, totalAnswers = 44)
        val running = SessionBank(learned = 4, correctAnswers = 41, totalAnswers = 45)

        val pending = banked.pending(running)

        assertEquals(1, pending.learned)
        assertEquals(1, pending.correctAnswers)
        assertEquals(1, pending.totalAnswers)
        assertFalse(pending.isEmpty)
    }

    @Test
    fun `re-banking unchanged totals writes nothing`() {
        val running = SessionBank(learned = 7, correctAnswers = 60, totalAnswers = 66)

        // First pass banks the lot; the watermark then advances to `running`.
        assertEquals(running, SessionBank().pending(running))

        // finish() re-banks the same session state — it must come back empty.
        assertTrue(running.pending(running).isEmpty)
    }

    @Test
    fun `banking every answer then finishing counts each word exactly once`() {
        // Replays a 4-word unit: every answer banks, and finish() banks once more.
        val answers = listOf(
            SessionBank(learned = 0, correctAnswers = 1, totalAnswers = 1),
            SessionBank(learned = 1, correctAnswers = 2, totalAnswers = 2),
            SessionBank(learned = 1, correctAnswers = 2, totalAnswers = 3),
            SessionBank(learned = 2, correctAnswers = 3, totalAnswers = 4),
        )

        var banked = SessionBank()
        var storedLearned = 0
        var storedCorrect = 0
        var storedAnswers = 0

        for (running in answers + answers.last()) { // trailing entry = finish()
            val pending = banked.pending(running)
            if (pending.isEmpty) continue
            storedLearned += pending.learned
            storedCorrect += pending.correctAnswers
            storedAnswers += pending.totalAnswers
            banked = running
        }

        val last = answers.last()
        assertEquals(last.learned, storedLearned)
        assertEquals(last.correctAnswers, storedCorrect)
        assertEquals(last.totalAnswers, storedAnswers)
    }

    @Test
    fun `a restarted session resets the watermark instead of banking negatives`() {
        // "One more group" builds a fresh session whose stats start over at zero.
        val previous = SessionBank(learned = 50, correctAnswers = 150, totalAnswers = 160)
        val restarted = SessionBank()
        val firstOfNewSession = SessionBank(learned = 1, correctAnswers = 3, totalAnswers = 3)

        assertEquals(firstOfNewSession, restarted.pending(firstOfNewSession))
        // Without the reset the delta would be negative and silently reduce today's row.
        assertEquals(-49, previous.pending(firstOfNewSession).learned)
    }
}
