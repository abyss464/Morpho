package dev.morpho.domain.learning

import dev.morpho.domain.model.LearnMode
import dev.morpho.domain.model.LearningProgress
import dev.morpho.domain.model.LearningStatus
import dev.morpho.domain.model.PlanWord
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNotNull
import kotlin.test.assertNull
import kotlin.test.assertTrue

class LearningEngineTest {

    private fun plan(count: Int, groupSize: Int = 18): List<PlanWord> =
        (1..count).map { i ->
            PlanWord(
                wordId = i.toLong(),
                groupId = ((i - 1) / groupSize + 1).toLong(),
                learningOrder = i,
            )
        }

    private val config = SessionConfig(sessionSeed = 20260101L)

    // -------------------------------------------------------------- planning

    @Test
    fun `session takes unlearned words in learning order up to the quota`() {
        val progress = mapOf(
            2L to LearningProgress(2, status = LearningStatus.LEARNED),
            5L to LearningProgress(5, status = LearningStatus.LEARNED),
        )
        val session = LearningEngine.buildSession(
            plan = plan(60).shuffled(),
            progress = progress,
            dueReviewIds = listOf(101L, 102L),
            newWordQuota = 10,
            config = config,
        )
        val ids = session.units.flatMap { it.wordIds }
        assertEquals(listOf(1L, 3L, 4L, 6L, 7L, 8L, 9L, 10L, 11L, 12L), ids)
        assertEquals(listOf(101L, 102L), session.reviewWordIds)
        assertEquals(10, session.newWordCount)
    }

    @Test
    fun `units close at group boundaries once they are big enough`() {
        // Groups of 18 words: each unit should be exactly one group.
        val units = LearningEngine.cutIntoUnits(plan(54, groupSize = 18), config)
        assertEquals(3, units.size)
        assertTrue(units.all { it.wordIds.size == 18 })
        assertEquals(listOf(1L, 2L, 3L), units.map { it.groupId })
    }

    @Test
    fun `oversized groups are split at the maximum unit size`() {
        val units = LearningEngine.cutIntoUnits(plan(50, groupSize = 50), config)
        assertEquals(listOf(20, 20, 10), units.map { it.wordIds.size })
    }

    @Test
    fun `a short remnant group merges with the words that follow it`() {
        // Group 1 has 4 words, group 2 has 20. A 4-word unit would be silly.
        val words = buildList {
            repeat(4) { add(PlanWord(it + 1L, 1L, it + 1)) }
            repeat(20) { add(PlanWord(it + 5L, 2L, it + 5)) }
        }
        val units = LearningEngine.cutIntoUnits(words, config)
        assertTrue(units.none { it.wordIds.size < 4 && it != units.last() })
        assertEquals(24, units.sumOf { it.wordIds.size })
        // The first unit crosses the boundary rather than closing at four words.
        assertTrue(units.first().wordIds.size >= 15)
    }

    // -------------------------------------------------------- mode ladder

    private fun answerAll(
        start: LearningSessionState,
        correct: (Question) -> Boolean,
        maxSteps: Int = 5000,
    ): LearningSessionState {
        var state = start
        var steps = 0
        while (!state.finished && state.currentQuestion != null) {
            val q = state.currentQuestion!!
            state = LearningEngine.submitAnswer(state, correct(q)).state
            if (++steps > maxSteps) error("engine did not terminate")
        }
        return state
    }

    private fun singleUnitSession(size: Int): LearningSessionState {
        val session = LearningEngine.buildSession(
            plan = plan(size, groupSize = size),
            progress = emptyMap(),
            dueReviewIds = emptyList(),
            newWordQuota = size,
            config = config,
        )
        return LearningEngine.startSession(session, emptyMap(), config)
    }

    @Test
    fun `three clean rounds promote through every mode and mark the word learned`() {
        var state = singleUnitSession(16)
        val seen = mutableListOf<LearnMode>()
        while (!state.finished && state.currentQuestion != null) {
            val q = state.currentQuestion!!
            if (q.wordId == 1L) seen += q.mode
            state = LearningEngine.submitAnswer(state, correct = true).state
        }
        assertEquals(
            listOf(LearnMode.SENTENCE_IMAGE, LearnMode.WORD_IMAGE_DEF, LearnMode.WORD_TEXT_DEF),
            seen,
        )
        assertEquals(16, state.learnedThisSession.size)
        assertEquals(16, state.stats.learned)
    }

    @Test
    fun `a wrong answer keeps the mode and forces a retry that banks nothing`() {
        var state = singleUnitSession(15)
        val firstQuestion = state.currentQuestion!!
        val target = firstQuestion.wordId

        val wrong = LearningEngine.submitAnswer(state, correct = false)
        assertTrue(wrong.outcome.mustRetry)
        assertTrue(wrong.outcome.showDetail)
        assertFalse(wrong.outcome.correct)

        val retry = wrong.state.currentQuestion!!
        assertEquals(target, retry.wordId, "retry must re-ask the same word")
        assertEquals(firstQuestion.mode, retry.mode, "a wrong answer must not change the mode")
        assertEquals(2, retry.attempt)

        val fixed = LearningEngine.submitAnswer(wrong.state, correct = true)
        assertFalse(fixed.outcome.promoted, "a retry must not promote")
        val card = fixed.state.current!!.cards.getValue(target)
        assertEquals(0, card.roundsPassed)
        assertEquals(firstQuestion.mode, card.mode)

        // ...and the same word comes back at the same mode next round.
        state = fixed.state
        var nextEncounter: Question? = null
        while (!state.finished && nextEncounter == null) {
            val q = state.currentQuestion ?: break
            if (q.wordId == target && q.round == 2) nextEncounter = q
            state = LearningEngine.submitAnswer(state, correct = true).state
        }
        assertNotNull(nextEncounter)
        assertEquals(LearnMode.SENTENCE_IMAGE, nextEncounter.mode)
    }

    @Test
    fun `option order is stable across a retry but differs between rounds`() {
        val state = singleUnitSession(15)
        val q1 = state.currentQuestion!!
        val afterWrong = LearningEngine.submitAnswer(state, correct = false).state
        val q1Retry = afterWrong.currentQuestion!!
        assertEquals(q1.optionSeed(config.sessionSeed), q1Retry.optionSeed(config.sessionSeed))

        val roundTwoSeed = q1.copy(round = 2).optionSeed(config.sessionSeed)
        assertTrue(roundTwoSeed != q1.optionSeed(config.sessionSeed))
    }

    // ----------------------------------------------------- carry-over / interleave

    @Test
    fun `words that miss a round carry into the next unit at their banked mode`() {
        val session = LearningEngine.buildSession(
            plan = plan(36, groupSize = 18),
            progress = emptyMap(),
            dueReviewIds = emptyList(),
            newWordQuota = 36,
            config = config,
        )
        assertEquals(2, session.units.size)
        val laggards = session.units[0].wordIds.take(3).toSet()

        var state = LearningEngine.startSession(session, emptyMap(), config)
        var carriedSeenInUnitOne = false
        while (!state.finished && state.currentQuestion != null) {
            val q = state.currentQuestion!!
            // Laggards fail their first attempt in round 1 only, then answer correctly.
            val correct = !(q.wordId in laggards && q.round == 1 && q.attempt == 1)
            val result = LearningEngine.submitAnswer(state, correct)
            if (result.outcome.unitCompleted && result.state.current?.index == 1) {
                val cards = result.state.current!!.cards
                for (id in laggards) {
                    val card = cards[id]
                    assertNotNull(card, "laggard $id was not carried into unit 2")
                    assertTrue(card.carried)
                    assertEquals(2, card.roundsPassed, "carried word must keep its banked rounds")
                    assertEquals(LearnMode.WORD_TEXT_DEF, card.mode)
                }
                carriedSeenInUnitOne = true
            }
            state = result.state
        }
        assertTrue(carriedSeenInUnitOne, "unit 1 never closed with carry-over")
        // Carried words finish inside unit 2 (one more banked round is enough).
        assertTrue(state.learnedThisSession.containsAll(laggards))
    }

    @Test
    fun `carried words are spread through the next unit, not clumped at the front`() {
        val order = interleave(primary = listOf(1, 2, 3, 4, 5, 6, 7, 8), secondary = listOf(90, 91))
        assertEquals(10, order.size)
        val positions = order.withIndex().filter { it.value >= 90 }.map { it.index }
        assertEquals(2, positions.size)
        assertTrue(positions.first() >= 3, "carried word appeared too early: $positions")
        assertTrue(positions.last() >= 8, "carried words clumped: $positions")
    }

    @Test
    fun `interleave keeps both sequences in order and preserves every element`() {
        val a = (1L..7L).toList()
        val b = (100L..103L).toList()
        val merged = interleave(a, b)
        assertEquals(a.size + b.size, merged.size)
        assertEquals(a, merged.filter { it < 100 })
        assertEquals(b, merged.filter { it >= 100 })
    }

    @Test
    fun `a word resuming from a previous day keeps its banked rounds`() {
        val prior = mapOf(
            1L to LearningProgress(1, LearnMode.WORD_TEXT_DEF, roundsPassed = 2),
        )
        val session = LearningEngine.buildSession(
            plan = plan(15, groupSize = 15),
            progress = prior,
            dueReviewIds = emptyList(),
            newWordQuota = 15,
            config = config,
        )
        var state = LearningEngine.startSession(session, prior, config)
        assertEquals(LearnMode.WORD_TEXT_DEF, state.current!!.cards.getValue(1L).mode)

        // One clean round is enough to graduate it; it must then drop out of round 2.
        var appearancesAfterGraduation = 0
        var graduated = false
        while (!state.finished && state.currentQuestion != null) {
            val q = state.currentQuestion!!
            if (graduated && q.wordId == 1L) appearancesAfterGraduation++
            val result = LearningEngine.submitAnswer(state, correct = true)
            if (q.wordId == 1L && result.outcome.wordLearned) graduated = true
            state = result.state
        }
        assertTrue(graduated)
        assertEquals(0, appearancesAfterGraduation, "a graduated word must stop being asked")
    }

    @Test
    fun `an unfinished word at the end of the day stays learning with its mode intact`() {
        var state = singleUnitSession(15)
        val stubborn = state.currentQuestion!!.wordId
        var lastUpdates = emptyList<LearningProgress>()
        while (!state.finished && state.currentQuestion != null) {
            val q = state.currentQuestion!!
            val correct = !(q.wordId == stubborn && q.attempt == 1)
            val result = LearningEngine.submitAnswer(state, correct)
            if (result.progressUpdates.any { it.wordId == stubborn }) {
                lastUpdates = result.progressUpdates
            }
            state = result.state
        }
        val row = lastUpdates.first { it.wordId == stubborn }
        assertEquals(LearningStatus.LEARNING, row.status)
        assertEquals(0, row.roundsPassed)
        assertEquals(LearnMode.SENTENCE_IMAGE, row.currentMode)
        assertFalse(stubborn in state.learnedThisSession)
        assertTrue(state.finished)
        assertNull(state.currentQuestion)
    }

    @Test
    fun `stats track first-attempt accuracy, not retries`() {
        var state = singleUnitSession(15)
        val stubborn = state.currentQuestion!!.wordId
        state = answerAll(state, correct = { q -> !(q.wordId == stubborn && q.attempt == 1) })
        // The stubborn word failed all three rounds; the other 14 passed three each.
        assertEquals(42, state.stats.correctFirstTry)
        assertEquals(3, state.stats.wrongFirstTry)
        assertEquals(14, state.stats.learned)
        assertEquals(0.9333, state.stats.accuracy!!, 0.001)
    }

    @Test
    fun `an empty plan yields a finished session`() {
        val session = LearningEngine.buildSession(
            plan = emptyList(),
            progress = emptyMap(),
            dueReviewIds = emptyList(),
            newWordQuota = 20,
            config = config,
        )
        assertTrue(session.isEmpty)
        val state = LearningEngine.startSession(session, emptyMap(), config)
        assertTrue(state.finished)
        assertNull(state.currentQuestion)
    }
}
