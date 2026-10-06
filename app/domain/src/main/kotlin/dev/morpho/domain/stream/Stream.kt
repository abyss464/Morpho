package dev.morpho.domain.stream

import dev.morpho.domain.model.FsrsCard
import dev.morpho.domain.review.FsrsScheduler
import dev.morpho.domain.review.Grade
import java.time.Instant
import java.time.LocalDate

/**
 * The stream: one mixed sequence of steps that takes every word from first sight to
 * long-term review. Implements docs/contracts/stream.md; the web client implements the
 * same file in `web/src/stream.ts`.
 */
enum class StepKind { KNOW, EXPLAIN1, EXPLAIN2, USE, REVIEW }

enum class ReviewTask { REBUILD, FILL }

enum class Outcome { CLEAN, SHAKY, FAILED }

enum class Stage { LEARNING, RELEARNING }

/** A word that is Learning (met, not graduated) or Relearning (a review was rated Again). */
data class WordStage(
    val stage: Stage,
    val next: StepKind,
    /** The next step follows at once (know -> explain1, failed -> know) rather than after spacing. */
    val immediate: Boolean,
    /** Stream position when [next] was scheduled. */
    val since: Int,
    /** Clean explain2 results still required before moving on. */
    val needClean: Int = 1,
    /** A Learning word goes on to `use` after its explain2 steps. */
    val thenUse: Boolean = true,
    /** Some step was shaky or failed: graduation rates Hard instead of Good. */
    val flawed: Boolean = false,
)

data class StreamDay(
    val date: LocalDate,
    val introduced: Int = 0,
    val extra: Int = 0,
    val steps: Int = 0,
    val reviewed: Int = 0,
    val reviewedClean: Int = 0,
    val met: Int = 0,
    val metClean: Int = 0,
    /** Position in the review, review, new rhythm. */
    val cycle: Int = 0,
)

data class Step(val wordId: Long, val kind: StepKind, val task: ReviewTask? = null)

/** The last review, so its derived rating can be replaced. */
data class LastReview(val wordId: Long, val prev: FsrsCard, val grade: Grade)

data class StreamState(
    val words: Map<Long, WordStage> = emptyMap(),
    val seq: Int = 0,
    val recent: List<StepKind> = emptyList(),
    val lastWord: Long? = null,
    val day: StreamDay,
    /** The step on screen, so a paused stream resumes on it. */
    val current: Step? = null,
    val lastReview: LastReview? = null,
)

/** What a finished step changed besides the stream state. */
data class StepResult(
    val state: StreamState,
    /** Cards to write: a graduated word's first card, or a reviewed card. */
    val cards: List<FsrsCard> = emptyList(),
    val graduated: Boolean = false,
    val reviewed: Boolean = false,
    /** The step was a graded task (explain, use, review), and whether it was clean. */
    val graded: Boolean = false,
    val clean: Boolean = false,
)

class StreamEngine(private val fsrs: FsrsScheduler) {

    companion object {
        /** Learning window, spacing between a word's steps, backlog guard (contract §5). */
        const val WINDOW = 5
        const val SPACING = 3
        const val BACKLOG = 50
        /** A rebuild review is the rebuild plus spelling the word: 10 s and 8 s. */
        const val REBUILD_EASY_MS = 18_000L
        const val FILL_EASY_MS = 4_000L
    }

    /** Today's counters, reset when the date changes. */
    fun today(state: StreamState, date: LocalDate): StreamState =
        if (state.day.date == date) state else state.copy(day = StreamDay(date))

    /** Words in review that are due, the most likely to be forgotten first. */
    fun dueReviews(state: StreamState, cards: Map<Long, FsrsCard>, shipped: Set<Long>, now: Instant): List<Long> =
        cards.values
            .filter { it.wordId !in state.words && it.wordId in shipped && !it.due.isAfter(now) }
            .sortedBy { fsrs.retrievability(it, now) }
            .map { it.wordId }

    fun nextNewWord(state: StreamState, order: List<Long>, cards: Map<Long, FsrsCard>): Long? =
        order.firstOrNull { it !in cards && it !in state.words }

    fun newAllowance(state: StreamState, newPerDay: Int): Int =
        (newPerDay + state.day.extra - state.day.introduced).coerceAtLeast(0)

    private fun reviewTask(card: FsrsCard?): ReviewTask =
        if (((card?.reps ?: 1) - 1) % 2 == 0) ReviewTask.REBUILD else ReviewTask.FILL

    /** Chooses the next step, or null when today's stream is done (contract §5). */
    fun next(
        input: StreamState,
        order: List<Long>,
        cards: Map<Long, FsrsCard>,
        newPerDay: Int,
        now: Instant,
        date: LocalDate,
    ): Step? {
        val s = today(input, date)
        val learning = s.words.entries.sortedBy { it.value.since }

        learning.firstOrNull { it.value.immediate }?.let { return Step(it.key, it.value.next) }

        val delayed = learning.filter { !it.value.immediate }
        val ready = delayed.filter { s.seq - it.value.since >= SPACING }
        val waiting = delayed.filter { s.seq - it.value.since < SPACING }

        val due = dueReviews(s, cards, order.toSet(), now)
        val fresh = if (s.words.size < WINDOW && newAllowance(s, newPerDay) > 0 && due.size < BACKLOG) {
            nextNewWord(s, order, cards)
        } else {
            null
        }
        val reviews = due.take(3).map { Step(it, StepKind.REVIEW, reviewTask(cards[it])) }
        val meet = listOfNotNull(fresh?.let { Step(it, StepKind.KNOW) })
        val mixed = if (s.day.cycle % 3 < 2) reviews + meet else meet + reviews

        val candidates = ready.map { Step(it.key, it.value.next) } + mixed + waiting.map { Step(it.key, it.value.next) }
        if (candidates.isEmpty()) return null

        // Guards: never the same word twice in a row; never four of one step type in a row.
        fun fine(c: Step) = c.wordId != s.lastWord && !(s.recent.size >= 3 && s.recent.all { it == c.kind })
        return candidates.firstOrNull(::fine) ?: candidates.first()
    }

    private fun stepsLeft(w: WordStage): Int {
        val after = if (w.stage == Stage.LEARNING && w.thenUse) 1 else 0
        return when (w.next) {
            StepKind.KNOW, StepKind.EXPLAIN1 -> 1 + w.needClean + after
            StepKind.EXPLAIN2 -> w.needClean + after
            StepKind.USE -> 1
            StepKind.REVIEW -> 1
        }
    }

    /** Steps done today and the estimated total (contract §5). */
    fun progress(
        input: StreamState,
        order: List<Long>,
        cards: Map<Long, FsrsCard>,
        newPerDay: Int,
        now: Instant,
        date: LocalDate,
    ): Pair<Int, Int> {
        val s = today(input, date)
        val due = dueReviews(s, cards, order.toSet(), now).size
        val pending = s.words.values.sumOf(::stepsLeft)
        val fresh = if (due < BACKLOG) newAllowance(s, newPerDay) else 0
        return s.day.steps to s.day.steps + due + pending + 4 * fresh
    }

    /** Rating derived from a review task (contract §6). */
    fun deriveGrade(outcome: Outcome, elapsedMs: Long, task: ReviewTask): Grade = when (outcome) {
        Outcome.FAILED -> Grade.AGAIN
        Outcome.SHAKY -> Grade.HARD
        Outcome.CLEAN ->
            if (elapsedMs < (if (task == ReviewTask.REBUILD) REBUILD_EASY_MS else FILL_EASY_MS)) Grade.EASY else Grade.GOOD
    }

    /** Applies a finished step (contract §3). */
    fun complete(
        input: StreamState,
        step: Step,
        outcome: Outcome,
        elapsedMs: Long,
        cards: Map<Long, FsrsCard>,
        now: Instant,
        date: LocalDate,
    ): StepResult {
        val s0 = today(input, date)
        val seq = s0.seq + 1
        val id = step.wordId
        val words = s0.words.toMutableMap()
        var day = s0.day.copy(steps = s0.day.steps + 1)
        val w = words[id]
        var changed = emptyList<FsrsCard>()
        var graduated = false
        var reviewed = false
        var lastReview = s0.lastReview
        fun later(next: StepKind, base: WordStage) = base.copy(next = next, immediate = false, since = seq)

        when (step.kind) {
            StepKind.KNOW -> if (w == null) {
                words[id] = WordStage(Stage.LEARNING, StepKind.EXPLAIN1, immediate = true, since = seq)
                day = day.copy(introduced = day.introduced + 1, cycle = day.cycle + 1)
            } else {
                words[id] = later(StepKind.EXPLAIN2, w)
            }

            StepKind.EXPLAIN1 -> words[id] = when (outcome) {
                Outcome.FAILED -> later(StepKind.EXPLAIN2, w!!).copy(needClean = 2, flawed = true)
                Outcome.SHAKY -> later(StepKind.EXPLAIN2, w!!).copy(flawed = true)
                Outcome.CLEAN -> later(StepKind.EXPLAIN2, w!!)
            }

            StepKind.EXPLAIN2 -> when (outcome) {
                Outcome.CLEAN -> {
                    val need = w!!.needClean - 1
                    when {
                        need > 0 -> words[id] = later(StepKind.EXPLAIN2, w).copy(needClean = need)
                        w.stage == Stage.LEARNING && w.thenUse -> words[id] = later(StepKind.USE, w).copy(needClean = 1)
                        else -> words.remove(id)
                    }
                }
                Outcome.SHAKY -> words[id] = later(StepKind.EXPLAIN2, w!!).copy(flawed = true)
                Outcome.FAILED -> words[id] = w!!.copy(next = StepKind.KNOW, immediate = true, since = seq, flawed = true)
            }

            StepKind.USE -> if (outcome == Outcome.CLEAN) {
                val grade = if (w!!.flawed) Grade.HARD else Grade.GOOD
                changed = listOf(fsrs.newCard(id, now, grade))
                words.remove(id)
                graduated = true
                day = day.copy(met = day.met + 1, metClean = day.metClean + if (w.flawed) 0 else 1)
            } else {
                words[id] = later(StepKind.EXPLAIN2, w!!).copy(needClean = 1, thenUse = true, flawed = true)
            }

            StepKind.REVIEW -> {
                val prev = cards[id] ?: error("review of word $id without a card")
                val grade = deriveGrade(outcome, elapsedMs, step.task ?: ReviewTask.REBUILD)
                changed = listOf(fsrs.review(prev, grade, now))
                reviewed = true
                day = day.copy(
                    reviewed = day.reviewed + 1,
                    reviewedClean = day.reviewedClean + if (outcome == Outcome.CLEAN) 1 else 0,
                    cycle = day.cycle + 1,
                )
                lastReview = LastReview(id, prev, grade)
                if (grade == Grade.AGAIN) words[id] = relearning(seq)
            }
        }

        val state = s0.copy(
            words = words,
            seq = seq,
            recent = (s0.recent + step.kind).takeLast(3),
            lastWord = id,
            day = day,
            current = null,
            lastReview = lastReview,
        )
        val graded = step.kind != StepKind.KNOW
        return StepResult(state, changed, graduated, reviewed, graded, graded && outcome == Outcome.CLEAN)
    }

    private fun relearning(seq: Int) = WordStage(
        Stage.RELEARNING, StepKind.KNOW, immediate = true, since = seq, needClean = 1, thenUse = false, flawed = true,
    )

    /** Replaces the derived rating of the last review with the learner's own choice. */
    fun override(state: StreamState, grade: Grade, now: Instant): StepResult {
        val last = state.lastReview ?: return StepResult(state)
        if (last.grade == grade) return StepResult(state)
        val words = state.words.toMutableMap()
        if (grade == Grade.AGAIN) words[last.wordId] = relearning(state.seq)
        else if (words[last.wordId]?.stage == Stage.RELEARNING) words.remove(last.wordId)
        return StepResult(
            state = state.copy(words = words, lastReview = last.copy(grade = grade)),
            cards = listOf(fsrs.review(last.prev, grade, now)),
        )
    }

    /** Days until the next review for each grade, from the card as it was before the review. */
    fun intervals(prev: FsrsCard, now: Instant): Map<Grade, Long> =
        Grade.entries.associateWith { g ->
            val next = fsrs.review(prev, g, now)
            java.time.Duration.between(now, next.due).toDays()
        }
}
