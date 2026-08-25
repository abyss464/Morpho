package dev.morpho.domain.learning

import dev.morpho.domain.model.LearnMode
import dev.morpho.domain.model.LearningProgress
import dev.morpho.domain.model.LearningStatus
import dev.morpho.domain.model.PlanWord
import dev.morpho.domain.util.deterministicShuffled
import dev.morpho.domain.util.seedOf

/**
 * Session building and the three-round mode ladder (README Part 1 "学习模式" and
 * Part 6 "组的短暂性").
 *
 * Invariants this file encodes:
 *
 *  * Due reviews come before new words; new words are taken strictly in
 *    `learning_order` from the words the user has not yet learned.
 *  * Groups are a **presentation** unit, never a progress unit. The engine holds no
 *    "current group" pointer: it rescans by `learning_order` every session, so a new
 *    content release that re-cuts groups is seamless.
 *  * Within a unit each word is asked once per round, three rounds. A first-attempt
 *    correct answer promotes the word one mode (1 -> 2 -> 3, capped) and banks a round.
 *    A wrong answer keeps the mode and burns the round, and the same question repeats
 *    until answered correctly (the retry never banks a round).
 *  * After three rounds, three banked rounds means learned; anything less keeps the
 *    word at its current mode and carries it, interleaved, into the next unit.
 */
object LearningEngine {

    // ---------------------------------------------------------------- planning

    /**
     * Splits the day's new words into 15-20 word units along group boundaries.
     *
     * [plan] must be the full shipped plan ordered by `learning_order`; [progress]
     * supplies the words already learned (skipped) and the in-flight mode/round state.
     */
    fun buildSession(
        plan: List<PlanWord>,
        progress: Map<Long, LearningProgress>,
        dueReviewIds: List<Long>,
        newWordQuota: Int,
        config: SessionConfig = SessionConfig(),
    ): SessionPlan {
        val candidates = plan
            .asSequence()
            .filter { progress[it.wordId]?.status != LearningStatus.LEARNED }
            .sortedBy { it.learningOrder }
            .take(newWordQuota.coerceAtLeast(0))
            .toList()

        return SessionPlan(
            reviewWordIds = dueReviewIds,
            units = cutIntoUnits(candidates, config),
        )
    }

    /**
     * Group-boundary chunking. A unit closes when it reaches [SessionConfig.maxUnitSize],
     * or at a group boundary once it is at least [SessionConfig.minUnitSize] long.
     * A short remnant group therefore merges with the words that follow it
     * ("残组与后续词自然合并") instead of producing a two-word unit.
     */
    internal fun cutIntoUnits(candidates: List<PlanWord>, config: SessionConfig): List<LearningUnit> {
        if (candidates.isEmpty()) return emptyList()
        val units = mutableListOf<LearningUnit>()
        var current = mutableListOf<PlanWord>()

        fun flush() {
            if (current.isEmpty()) return
            units += LearningUnit(
                index = units.size,
                groupId = current.first().groupId,
                wordIds = current.map { it.wordId },
            )
            current = mutableListOf()
        }

        for ((i, word) in candidates.withIndex()) {
            current += word
            val next = candidates.getOrNull(i + 1)
            val atGroupBoundary = next == null || next.groupId != word.groupId
            val full = current.size >= config.maxUnitSize
            if (full || (atGroupBoundary && current.size >= config.minUnitSize) || next == null) {
                flush()
            }
        }
        flush()
        return units
    }

    // ------------------------------------------------------------- session run

    /** Builds the initial runtime state for a planned session. */
    fun startSession(
        plan: SessionPlan,
        progress: Map<Long, LearningProgress>,
        config: SessionConfig = SessionConfig(),
    ): LearningSessionState {
        val state = LearningSessionState(
            config = config,
            units = plan.units,
            unitIndex = 0,
            current = null,
            carryOver = emptyList(),
            learnedThisSession = emptyList(),
            stats = SessionStats(),
            finished = plan.units.isEmpty(),
            progressSnapshot = progress,
        )
        return if (plan.units.isEmpty()) state else state.enterUnit(0)
    }

    /**
     * Records an answer to [LearningSessionState.currentQuestion] and returns the next state
     * together with everything the caller must persist and animate.
     */
    fun submitAnswer(state: LearningSessionState, correct: Boolean): AnswerResult {
        val runtime = state.current ?: return AnswerResult(state, AnswerOutcome.noop())
        val question = state.currentQuestion ?: return AnswerResult(state, AnswerOutcome.noop())

        val card = runtime.cards.getValue(question.wordId)
        val firstAttempt = runtime.attempt == 1

        if (!correct) {
            // Wrong: keep the mode, burn the round, and repeat the same question.
            val updated = if (firstAttempt) card.copy(roundFailed = true) else card
            val nextRuntime = runtime.copy(
                cards = runtime.cards + (card.wordId to updated),
                attempt = runtime.attempt + 1,
            )
            return AnswerResult(
                state = state.copy(
                    current = nextRuntime,
                    stats = state.stats.copy(
                        answered = state.stats.answered + 1,
                        wrongFirstTry = state.stats.wrongFirstTry + if (firstAttempt) 1 else 0,
                    ),
                ),
                outcome = AnswerOutcome(
                    correct = false,
                    firstAttempt = firstAttempt,
                    mustRetry = true,
                    showDetail = true,
                ),
                progressUpdates = listOf(updated.toProgress()),
            )
        }

        // Correct. Only a clean first attempt banks a round and promotes the mode.
        val banked = firstAttempt && !card.roundFailed
        val promotedMode = if (banked) card.mode.promoted() else card.mode
        var updatedCard = card.copy(
            mode = promotedMode,
            roundsPassed = card.roundsPassed + if (banked) 1 else 0,
            roundFailed = false,
        )

        val reachedLearned = updatedCard.roundsPassed >= state.config.roundsRequired
        if (reachedLearned) {
            updatedCard = updatedCard.copy(retired = true)
        }

        var stats = state.stats.copy(
            answered = state.stats.answered + 1,
            correctFirstTry = state.stats.correctFirstTry + if (banked) 1 else 0,
            promotions = state.stats.promotions + if (banked && promotedMode != card.mode) 1 else 0,
        )

        var learned = state.learnedThisSession
        if (reachedLearned) {
            learned = learned + updatedCard.wordId
            stats = stats.copy(learned = stats.learned + 1)
        }

        var runtimeAfter = runtime.copy(
            cards = runtime.cards + (card.wordId to updatedCard),
            cursor = runtime.cursor + 1,
            attempt = 1,
        )

        var next = state.copy(
            current = runtimeAfter,
            learnedThisSession = learned,
            stats = stats,
        )

        val outcomeBase = AnswerOutcome(
            correct = true,
            firstAttempt = firstAttempt,
            promoted = banked && promotedMode != card.mode,
            newMode = promotedMode,
            wordLearned = reachedLearned,
            showDetail = reachedLearned,
        )

        val singleUpdate = listOf(updatedCard.toProgress())

        // Round / unit boundaries.
        if (runtimeAfter.cursor < runtimeAfter.order.size) {
            return AnswerResult(next, outcomeBase, singleUpdate)
        }

        if (runtimeAfter.round < state.config.roundsRequired) {
            val nextRound = runtimeAfter.round + 1
            val active = runtimeAfter.activeCards().map { it.wordId }
            if (active.isNotEmpty()) {
                runtimeAfter = runtimeAfter.copy(
                    round = nextRound,
                    cursor = 0,
                    attempt = 1,
                    order = active.deterministicShuffled(
                        seedOf(
                            state.config.sessionSeed,
                            runtimeAfter.index.toLong(),
                            nextRound.toLong(),
                        ),
                    ),
                )
                next = next.copy(current = runtimeAfter)
                return AnswerResult(next, outcomeBase.copy(roundCompleted = true), singleUpdate)
            }
        }

        // Unit is over: graduate what banked all rounds, carry the rest.
        val closingCards = runtimeAfter.cards.values.map { it.toProgress() }
        val closed = next.closeUnit()
        return AnswerResult(
            state = closed,
            outcome = outcomeBase.copy(
                roundCompleted = true,
                unitCompleted = true,
                sessionCompleted = closed.finished,
            ),
            progressUpdates = closingCards,
        )
    }

    /**
     * Skips ahead — used by "give up / end session". Persists whatever has been banked.
     */
    fun abandon(state: LearningSessionState): LearningSessionState =
        state.copy(current = null, finished = true)
}

// -------------------------------------------------------------------- types

data class SessionConfig(
    val dailyGoal: Int = 50,
    val minUnitSize: Int = 15,
    val maxUnitSize: Int = 20,
    val roundsRequired: Int = 3,
    val sessionSeed: Long = 0L,
)

data class LearningUnit(
    val index: Int,
    val groupId: Long,
    /** New (never-learned) words native to this unit, in `learning_order`. */
    val wordIds: List<Long>,
)

data class SessionPlan(
    val reviewWordIds: List<Long> = emptyList(),
    val units: List<LearningUnit> = emptyList(),
) {
    val newWordCount: Int get() = units.sumOf { it.wordIds.size }
    val reviewCount: Int get() = reviewWordIds.size
    val isEmpty: Boolean get() = reviewWordIds.isEmpty() && units.isEmpty()
}

/** Per-word state inside a running unit. */
data class WordCard(
    val wordId: Long,
    val mode: LearnMode,
    val roundsPassed: Int,
    /** True once the word has been answered wrong in the current round. */
    val roundFailed: Boolean = false,
    /** Carried in from an earlier unit that it did not clear. */
    val carried: Boolean = false,
    /** Reached three banked rounds; dropped from any remaining rounds. */
    val retired: Boolean = false,
) {
    fun toProgress(): LearningProgress = LearningProgress(
        wordId = wordId,
        currentMode = mode,
        roundsPassed = roundsPassed,
        status = if (retired) LearningStatus.LEARNED else LearningStatus.LEARNING,
    )
}

data class UnitRuntime(
    val index: Int,
    val groupId: Long,
    val cards: Map<Long, WordCard>,
    /** Presentation order for the current round. */
    val order: List<Long>,
    val round: Int,
    val cursor: Int,
    val attempt: Int,
) {
    fun activeCards(): List<WordCard> = cards.values.filterNot { it.retired }

    val currentWordId: Long? get() = order.getOrNull(cursor)
}

data class Question(
    val wordId: Long,
    val mode: LearnMode,
    val round: Int,
    val unitIndex: Int,
    val positionInRound: Int,
    val roundSize: Int,
    val attempt: Int,
) {
    /**
     * Seed for the option shuffle. Stable per question instance: the same word in the
     * same round of the same unit always yields the same option order, including on
     * a retry after a wrong answer.
     */
    fun optionSeed(sessionSeed: Long): Long =
        seedOf(sessionSeed, wordId, mode.level.toLong(), round.toLong(), unitIndex.toLong())
}

data class SessionStats(
    val answered: Int = 0,
    val correctFirstTry: Int = 0,
    val wrongFirstTry: Int = 0,
    val promotions: Int = 0,
    val learned: Int = 0,
    val reviewed: Int = 0,
    val reviewCorrect: Int = 0,
) {
    val firstTryTotal: Int get() = correctFirstTry + wrongFirstTry
    val accuracy: Double?
        get() = if (firstTryTotal == 0) null else correctFirstTry.toDouble() / firstTryTotal
}

data class AnswerOutcome(
    val correct: Boolean = false,
    val firstAttempt: Boolean = false,
    val mustRetry: Boolean = false,
    val promoted: Boolean = false,
    val newMode: LearnMode = LearnMode.SENTENCE_IMAGE,
    val wordLearned: Boolean = false,
    val roundCompleted: Boolean = false,
    val unitCompleted: Boolean = false,
    val sessionCompleted: Boolean = false,
    /** Wrong answers force the detail sheet; so does a word graduating. */
    val showDetail: Boolean = false,
) {
    companion object {
        fun noop() = AnswerOutcome()
    }
}

data class AnswerResult(
    val state: LearningSessionState,
    val outcome: AnswerOutcome,
    /** Rows the caller must write to `learning_progress`. */
    val progressUpdates: List<LearningProgress> = emptyList(),
)

data class LearningSessionState(
    val config: SessionConfig,
    val units: List<LearningUnit>,
    val unitIndex: Int,
    val current: UnitRuntime?,
    val carryOver: List<WordCard>,
    val learnedThisSession: List<Long>,
    val stats: SessionStats,
    val finished: Boolean,
    /**
     * Progress rows as they stood when the session started, kept so that a later unit
     * can restore a word that was already part-way through its rounds on a previous day.
     * Runtime cards are layered on top as units close.
     */
    val progressSnapshot: Map<Long, LearningProgress> = emptyMap(),
) {
    val currentQuestion: Question?
        get() {
            val runtime = current ?: return null
            val wordId = runtime.currentWordId ?: return null
            val card = runtime.cards[wordId] ?: return null
            return Question(
                wordId = wordId,
                mode = card.mode,
                round = runtime.round,
                unitIndex = runtime.index,
                positionInRound = runtime.cursor,
                roundSize = runtime.order.size,
                attempt = runtime.attempt,
            )
        }

    /** Words in the current unit ordered for the GroupProgressBar. */
    fun unitProgress(): List<WordCard> {
        val runtime = current ?: return emptyList()
        val native = units.getOrNull(runtime.index)?.wordIds.orEmpty()
        val ordered = LinkedHashSet<Long>().apply {
            addAll(runtime.cards.keys.filter { it !in native })
            addAll(native)
        }
        return ordered.mapNotNull { runtime.cards[it] }
    }

    internal fun enterUnit(index: Int): LearningSessionState {
        val unit = units.getOrNull(index) ?: return copy(current = null, finished = true)
        val nativeCards = unit.wordIds.map { id ->
            val p = progressSnapshot[id]
            WordCard(
                wordId = id,
                mode = p?.currentMode ?: LearnMode.SENTENCE_IMAGE,
                roundsPassed = p?.roundsPassed ?: 0,
            )
        }
        val carried = carryOver.map { it.copy(carried = true, roundFailed = false) }
        val cards = (carried + nativeCards).associateBy { it.wordId }

        // Interleave carried words evenly through the new ones so the user meets them
        // spread across the unit rather than in a clump at the start.
        val order = interleave(
            primary = nativeCards.map { it.wordId }
                .deterministicShuffled(seedOf(config.sessionSeed, index.toLong(), 1L)),
            secondary = carried.map { it.wordId }
                .deterministicShuffled(seedOf(config.sessionSeed, index.toLong(), 101L)),
        )

        return copy(
            unitIndex = index,
            current = UnitRuntime(
                index = index,
                groupId = unit.groupId,
                cards = cards,
                order = order,
                round = 1,
                cursor = 0,
                attempt = 1,
            ),
            carryOver = emptyList(),
            finished = false,
        )
    }

    /**
     * Ends the current unit: graduates the words that banked all three rounds and
     * queues the rest for the next unit.
     */
    internal fun closeUnit(): LearningSessionState {
        val runtime = current ?: return copy(finished = true)
        val unfinished = runtime.cards.values
            .filterNot { it.retired }
            .sortedBy { it.wordId }
        val nextIndex = unitIndex + 1
        val base = copy(
            carryOver = unfinished,
            progressSnapshot = progressSnapshot +
                runtime.cards.mapValues { (_, card) -> card.toProgress() },
        )
        return if (nextIndex < units.size) {
            base.enterUnit(nextIndex)
        } else {
            // No more planned units today: unfinished words keep their banked mode in
            // user.db and resume tomorrow from the same point.
            base.copy(current = null, finished = true)
        }
    }
}

/**
 * Evenly distributes [secondary] through [primary] while keeping the relative order
 * of both. With 4 primaries and 2 secondaries the result is `p p s p p s`.
 */
internal fun interleave(primary: List<Long>, secondary: List<Long>): List<Long> {
    if (secondary.isEmpty()) return primary
    if (primary.isEmpty()) return secondary
    val total = primary.size + secondary.size
    val out = ArrayList<Long>(total)
    var pi = 0
    var si = 0
    for (k in 0 until total) {
        val secondaryIsDue = (si + 1).toLong() * total <= (k + 1).toLong() * secondary.size
        when {
            secondaryIsDue && si < secondary.size -> out += secondary[si++]
            pi < primary.size -> out += primary[pi++]
            else -> out += secondary[si++]
        }
    }
    return out
}
