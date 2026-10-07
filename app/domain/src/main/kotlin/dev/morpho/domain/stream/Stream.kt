package dev.morpho.domain.stream

import dev.morpho.domain.model.FsrsCard
import dev.morpho.domain.review.FsrsScheduler
import dev.morpho.domain.review.Grade
import java.time.Duration
import java.time.Instant
import java.time.LocalDate
import java.time.ZoneId

/**
 * The stream: one mixed sequence of steps that takes every word from first sight to
 * long-term review. Implements docs/contracts/stream.md; the web client implements the
 * same file in `web/src/stream.ts`.
 */
enum class StepKind { KNOW, EXPLAIN, SPELL, USE, REVIEW }

enum class ReviewTask { REBUILD, FILL }

enum class Outcome {
    CLEAN,
    SHAKY,
    FAILED,
    ;

    val passed: Boolean get() = this != FAILED

    companion object {
        /**
         * A task's outcome from its mistakes (contract §2): none is clean, one is shaky, two or
         * more fail it, and so does showing the answer.
         */
        fun of(mistakes: Int, answerShown: Boolean): Outcome = when {
            answerShown || mistakes >= 2 -> FAILED
            mistakes == 1 -> SHAKY
            else -> CLEAN
        }
    }
}

/** A word that is Learning: met, not graduated. */
data class WordStage(
    val next: StepKind,
    /** The next step follows at once (know -> explain -> spell) rather than after spacing. */
    val immediate: Boolean,
    /** Stream position when [next] was scheduled. */
    val since: Int,
    /** Some step was shaky or failed: graduation rates Hard instead of Good. */
    val flawed: Boolean = false,
    /** How many times [next] has failed; a step done again is shuffled with this. */
    val attempt: Int = 0,
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

/**
 * One step of the stream. [attempt] is the word's attempt at a learning step: 0 the first
 * time, more when the step is done again after a failure, so it is shuffled anew.
 */
data class Step(val wordId: Long, val kind: StepKind, val task: ReviewTask? = null, val attempt: Int = 0)

/**
 * The seed a step's shuffle uses: the task's own seed for the first attempt, moved on by a
 * fixed stride for each later one (the web client seeds the same way).
 */
fun attemptSeed(seed: Long, attempt: Int): Long = seed + attempt * ATTEMPT_STRIDE

private const val ATTEMPT_STRIDE = 7919L

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
    /** The step was a graded task (explain, spell, use, review), and whether it was clean. */
    val graded: Boolean = false,
    val clean: Boolean = false,
)

/**
 * @param zone the learner's time zone: a graduated word's first review and a review rated
 *   Again are due at the start of the next local day (contract §6)
 */
class StreamEngine(
    private val fsrs: FsrsScheduler,
    private val zone: ZoneId = ZoneId.systemDefault(),
) {

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

    private fun stepOf(id: Long, w: WordStage) = Step(id, w.next, attempt = w.attempt)

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

        learning.firstOrNull { it.value.immediate }?.let { return stepOf(it.key, it.value) }

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

        val candidates = ready.map { stepOf(it.key, it.value) } + mixed + waiting.map { stepOf(it.key, it.value) }
        if (candidates.isEmpty()) return null

        // Guards: never the same word twice in a row; never four of one step type in a row.
        fun fine(c: Step) = c.wordId != s.lastWord && !(s.recent.size >= 3 && s.recent.all { it == c.kind })
        return candidates.firstOrNull(::fine) ?: candidates.first()
    }

    /** Steps a Learning word still has to take, each passed once. */
    private fun stepsLeft(w: WordStage): Int = when (w.next) {
        StepKind.KNOW -> 4
        StepKind.EXPLAIN -> 3
        StepKind.SPELL -> 2
        StepKind.USE, StepKind.REVIEW -> 1
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

        when (step.kind) {
            StepKind.KNOW -> {
                if (w == null) day = day.copy(introduced = day.introduced + 1, cycle = day.cycle + 1)
                words[id] = WordStage(StepKind.EXPLAIN, immediate = true, since = seq, flawed = w?.flawed ?: false)
            }

            StepKind.EXPLAIN, StepKind.SPELL, StepKind.USE -> {
                val stage = w ?: error("${step.kind} of word $id, which is not being learned")
                val flawed = stage.flawed || outcome != Outcome.CLEAN
                when {
                    // Done again a few steps later, shuffled anew.
                    !outcome.passed ->
                        words[id] = stage.copy(immediate = false, since = seq, flawed = true, attempt = stage.attempt + 1)
                    step.kind == StepKind.EXPLAIN ->
                        words[id] = WordStage(StepKind.SPELL, immediate = true, since = seq, flawed = flawed)
                    step.kind == StepKind.SPELL ->
                        words[id] = WordStage(StepKind.USE, immediate = false, since = seq, flawed = flawed)
                    else -> {
                        val first = fsrs.newCard(id, now, if (flawed) Grade.HARD else Grade.GOOD)
                        changed = listOf(dueTomorrow(first, date))
                        words.remove(id)
                        graduated = true
                        day = day.copy(met = day.met + 1, metClean = day.metClean + if (flawed) 0 else 1)
                    }
                }
            }

            StepKind.REVIEW -> {
                val prev = cards[id] ?: error("review of word $id without a card")
                val grade = deriveGrade(outcome, elapsedMs, step.task ?: ReviewTask.REBUILD)
                changed = listOf(schedule(prev, grade, now))
                reviewed = true
                day = day.copy(
                    reviewed = day.reviewed + 1,
                    reviewedClean = day.reviewedClean + if (outcome == Outcome.CLEAN) 1 else 0,
                    cycle = day.cycle + 1,
                )
                lastReview = LastReview(id, prev, grade)
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

    /** Replaces the derived rating of the last review with the learner's own choice. */
    fun override(state: StreamState, grade: Grade, now: Instant): StepResult {
        val last = state.lastReview ?: return StepResult(state)
        if (last.grade == grade) return StepResult(state)
        return StepResult(
            state = state.copy(lastReview = last.copy(grade = grade)),
            cards = listOf(schedule(last.prev, grade, now)),
        )
    }

    /**
     * Whole days until the next review for each grade, at least 1, from the card as it was
     * before the review (contract §6).
     */
    fun intervals(prev: FsrsCard, now: Instant): Map<Grade, Long> =
        Grade.entries.associateWith { g ->
            val ms = Duration.between(now, schedule(prev, g, now).due).toMillis()
            Math.round(ms / DAY_MS).coerceAtLeast(1L)
        }

    /** A review of [prev] rated [grade]: FSRS's card, due the next day when rated Again. */
    private fun schedule(prev: FsrsCard, grade: Grade, now: Instant): FsrsCard {
        val card = fsrs.review(prev, grade, now)
        return if (grade == Grade.AGAIN) dueTomorrow(card, now.atZone(zone).toLocalDate()) else card
    }

    /** [card] due at the start of the local day after [date]. */
    private fun dueTomorrow(card: FsrsCard, date: LocalDate): FsrsCard =
        card.copy(due = date.plusDays(1).atStartOfDay(zone).toInstant(), scheduledDays = 1)
}

private const val DAY_MS = 86_400_000.0
