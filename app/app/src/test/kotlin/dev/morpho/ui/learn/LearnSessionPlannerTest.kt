package dev.morpho.ui.learn

import dev.morpho.domain.learning.LearningEngine
import dev.morpho.domain.learning.SessionConfig
import dev.morpho.domain.model.PlanWord
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNull
import kotlin.test.assertTrue

/**
 * The pre-session decisions of [LearnViewModel]: a met daily goal must ask before it
 * spends more words, "one more group" must cost exactly one group, and a plan with
 * nothing in it must not pretend a session finished.
 */
class LearnSessionPlannerTest {

    private val config = SessionConfig(sessionSeed = 20260827L)

    private fun plan(count: Int, groupSize: Int = 18): List<PlanWord> =
        (1..count).map { i ->
            PlanWord(
                wordId = i.toLong(),
                groupId = ((i - 1) / groupSize + 1).toLong(),
                learningOrder = i,
            )
        }

    // ----------------------------------------------------------------- quota

    @Test
    fun `an untouched day is worth the whole goal`() {
        assertEquals(50, LearnSessionPlanner.quotaFor(dailyGoal = 50, doneToday = 0))
    }

    @Test
    fun `a partly used day is worth what is left of it`() {
        assertEquals(30, LearnSessionPlanner.quotaFor(dailyGoal = 50, doneToday = 20))
        assertEquals(1, LearnSessionPlanner.quotaFor(dailyGoal = 50, doneToday = 49))
    }

    @Test
    fun `a met goal has no quota - the user is asked instead of refilled`() {
        // The old behaviour handed out another full dailyGoal batch here.
        assertNull(LearnSessionPlanner.quotaFor(dailyGoal = 50, doneToday = 50))
        assertNull(LearnSessionPlanner.quotaFor(dailyGoal = 50, doneToday = 63))
    }

    // ------------------------------------------------------------- one group

    @Test
    fun `one more group is a single unit, not another daily batch`() {
        val built = LearningEngine.buildSession(
            plan = plan(200),
            progress = emptyMap(),
            dueReviewIds = emptyList(),
            newWordQuota = LearnSessionPlanner.extraGroupQuota(config),
            config = config,
        )
        val trimmed = LearnSessionPlanner.singleUnit(built)

        assertEquals(1, trimmed.units.size)
        assertTrue(
            trimmed.newWordCount in config.minUnitSize..config.maxUnitSize,
            "one group is 15-20 words, got ${trimmed.newWordCount}",
        )
        assertTrue(trimmed.newWordCount < config.dailyGoal)
        // Taken from the front of learning_order, like any other session.
        assertEquals(1L, trimmed.units.first().wordIds.first())
    }

    @Test
    fun `a group boundary inside the quota still leaves exactly one unit`() {
        // Groups of 16: the quota of 20 cuts a 16-word unit plus a 4-word remnant.
        val built = LearningEngine.buildSession(
            plan = plan(200, groupSize = 16),
            progress = emptyMap(),
            dueReviewIds = emptyList(),
            newWordQuota = LearnSessionPlanner.extraGroupQuota(config),
            config = config,
        )
        assertEquals(2, built.units.size)
        val trimmed = LearnSessionPlanner.singleUnit(built)
        assertEquals(1, trimmed.units.size)
        assertEquals(16, trimmed.newWordCount)
    }

    @Test
    fun `trimming an empty plan leaves it empty`() {
        val built = LearningEngine.buildSession(
            plan = emptyList(),
            progress = emptyMap(),
            dueReviewIds = emptyList(),
            newWordQuota = LearnSessionPlanner.extraGroupQuota(config),
            config = config,
        )
        assertTrue(LearnSessionPlanner.singleUnit(built).units.isEmpty())
    }

    // ----------------------------------------------------------- entry states

    @Test
    fun `an empty plan does not finish, so the screen never opens the summary`() {
        // LearnScreen navigates to the summary on `finished`; with no session there is
        // no SessionResult to show and it would render the previous session's numbers.
        val state = LearnSessionPlanner.emptyState()
        assertFalse(state.finished)
        assertTrue(state.empty)
        assertFalse(state.loading)
        assertNull(state.question)
    }

    @Test
    fun `the goal-reached prompt does not finish either`() {
        val state = LearnSessionPlanner.goalReachedState()
        assertTrue(state.goalReached)
        assertFalse(state.finished)
        assertFalse(state.loading)
        assertFalse(state.empty)
    }
}
