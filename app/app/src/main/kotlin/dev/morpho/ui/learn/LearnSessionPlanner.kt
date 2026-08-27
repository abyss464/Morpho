package dev.morpho.ui.learn

import dev.morpho.domain.learning.SessionConfig
import dev.morpho.domain.learning.SessionPlan

/**
 * The decisions the learn screen makes *before* a session exists: how many new words a
 * start is worth today, and how much of a built plan an extra run past the daily goal
 * keeps. Pure, so they can be tested without a container.
 *
 * The day boundary is the caller's `LocalDate.now()` against today's stats row, so the
 * quota resets at local midnight on its own — nothing here carries a clock.
 */
internal object LearnSessionPlanner {

    /**
     * New-word quota for a normal start, or `null` when today's goal is already met.
     * A met goal must not silently refill another full batch (backlog #22): the caller
     * asks the user first.
     */
    fun quotaFor(dailyGoal: Int, doneToday: Int): Int? {
        val remaining = dailyGoal - doneToday
        return if (remaining <= 0) null else remaining
    }

    /**
     * Quota to build from when the user answers the prompt with "one more group". One
     * unit is at most [SessionConfig.maxUnitSize] words, so building that many and
     * keeping the first unit ([singleUnit]) yields exactly one group's worth.
     */
    fun extraGroupQuota(config: SessionConfig): Int = config.maxUnitSize

    /**
     * Trims a plan built with [extraGroupQuota] to a single unit: "one more group" is one
     * 15-20 word unit, never another daily batch. A group boundary inside the quota can
     * cut a second, short unit; that remnant belongs to tomorrow.
     */
    fun singleUnit(plan: SessionPlan): SessionPlan = plan.copy(units = plan.units.take(1))

    /**
     * Nothing left to teach. Deliberately **not** `finished`: finishing is what publishes
     * a [dev.morpho.di.SessionResult] and sends the screen to the summary, and an empty
     * plan has no result to show — the summary would render the previous session's
     * numbers (backlog #23).
     */
    fun emptyState(): LearnUiState = LearnUiState(loading = false, empty = true)

    /** Goal met: ask before spending another group. Also not `finished`, same reason. */
    fun goalReachedState(): LearnUiState = LearnUiState(loading = false, goalReached = true)
}
