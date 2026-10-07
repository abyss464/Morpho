package dev.morpho.data.stream

import android.util.Log
import dev.morpho.data.repository.ContentRepository
import dev.morpho.data.repository.ProgressRepository
import dev.morpho.domain.model.CardState
import dev.morpho.domain.model.DailyStats
import dev.morpho.domain.model.FsrsCard
import dev.morpho.domain.model.UserMetaKeys
import dev.morpho.domain.review.Grade
import dev.morpho.domain.stream.LastReview
import dev.morpho.domain.stream.ReviewTask
import dev.morpho.domain.stream.Step
import dev.morpho.domain.stream.StepKind
import dev.morpho.domain.stream.StepResult
import dev.morpho.domain.stream.StreamDay
import dev.morpho.domain.stream.StreamState
import dev.morpho.domain.stream.WordStage
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import java.time.Instant
import java.time.LocalDate

/** The learner's own explanation of a word, written after a review. */
data class WordNote(val text: String, val at: Instant)

/** Everything the stream engine reads, loaded together. */
data class StreamSnapshot(
    /** Every shipped word id, in release learning order. */
    val order: List<Long>,
    val cards: Map<Long, FsrsCard>,
    val state: StreamState,
) {
    private val position: Map<Long, Int> by lazy { order.withIndex().associate { (i, id) -> id to i } }

    /** 1-based unit of [wordId]: [UNIT_SIZE] words per unit in learning order. */
    fun unitOf(wordId: Long): Int = (position[wordId] ?: 0) / UNIT_SIZE + 1

    /** Learning-order position of [wordId], or -1 when this release does not ship it. */
    fun positionOf(wordId: Long): Int = position[wordId] ?: -1

    companion object {
        /** Words per unit (docs/contracts/stream.md §7). */
        const val UNIT_SIZE = 20
    }
}

/**
 * Persistence of the stream (docs/contracts/stream.md) inside user.db without a schema
 * change: the [StreamState] and the learner's notes are JSON values in the `meta` table,
 * so they ride along with Auto Backup and the manual export like every other row there.
 */
class StreamStore(
    private val progress: ProgressRepository,
    private val content: ContentRepository,
) {

    private val json = Json { ignoreUnknownKeys = true }

    /** The learning order, every card and the stream state, as of [today]. */
    suspend fun open(today: LocalDate): StreamSnapshot {
        val order = content.planWords().map { it.wordId }
        val cards = progress.allCards().associateBy { it.wordId }
        return StreamSnapshot(order, cards, load(order.toSet(), cards, today))
    }

    /**
     * The saved stream, or a fresh one built from what the app already knows: words with an
     * FSRS card stay in review, and words the learning ladder left mid-way (`learning` with a
     * round passed) continue as Learning, due for a delayed `explain`.
     */
    private suspend fun load(shipped: Set<Long>, cards: Map<Long, FsrsCard>, today: LocalDate): StreamState {
        progress.metaValue(UserMetaKeys.STREAM_STATE)?.let { raw ->
            runCatching { json.decodeFromString<StoredState>(raw).toDomain() }
                .onSuccess { return it }
                .onFailure { Log.e(TAG, "unreadable stream state, starting from progress", it) }
        }
        val carried = progress.inFlightWordIds()
            .filter { it in shipped && it !in cards }
            .associateWith { WordStage(StepKind.EXPLAIN, immediate = false, since = 0) }
        val state = StreamState(words = carried, day = StreamDay(today))
        save(state)
        return state
    }

    suspend fun save(state: StreamState) {
        progress.setMeta(UserMetaKeys.STREAM_STATE, encode(state))
    }

    /**
     * Writes a finished step: its cards, the day's counts (`new_learned` += graduated,
     * `reviewed` += reviewed, `answer_count` += graded, `correct_count` += clean) and the
     * new stream state, in one transaction.
     */
    suspend fun record(result: StepResult, date: LocalDate) {
        progress.recordStep(
            changed = result.cards,
            delta = DailyStats(
                date = date,
                newLearned = if (result.graduated) 1 else 0,
                reviewed = if (result.reviewed) 1 else 0,
                correctCount = if (result.clean) 1 else 0,
                answerCount = if (result.graded) 1 else 0,
            ),
            metaRows = mapOf(UserMetaKeys.STREAM_STATE to encode(result.state)),
        )
    }

    suspend fun notes(): Map<Long, WordNote> {
        val raw = progress.metaValue(UserMetaKeys.STREAM_NOTES) ?: return emptyMap()
        return runCatching { json.decodeFromString<Map<Long, StoredNote>>(raw) }
            .onFailure { Log.e(TAG, "unreadable notes", it) }
            .getOrDefault(emptyMap())
            .mapValues { (_, n) -> WordNote(n.text, Instant.parse(n.at)) }
    }

    /** Saves [text] as the note for [wordId], replacing any earlier one. */
    suspend fun saveNote(wordId: Long, text: String, now: Instant): Map<Long, WordNote> {
        val notes = notes() + (wordId to WordNote(text.trim(), now))
        progress.setMeta(UserMetaKeys.STREAM_NOTES, encodeNotes(notes))
        return notes
    }

    /**
     * Writes what a sync brought in, in one transaction: the [cards] it set, the stream
     * [state] and every [notes]. Today's counters and `daily_stats` are not touched.
     */
    suspend fun replace(cards: Collection<FsrsCard>, state: StreamState, notes: Map<Long, WordNote>) {
        progress.recordStep(
            changed = cards,
            delta = DailyStats(date = state.day.date),
            metaRows = mapOf(
                UserMetaKeys.STREAM_STATE to encode(state),
                UserMetaKeys.STREAM_NOTES to encodeNotes(notes),
            ),
        )
    }

    private fun encodeNotes(notes: Map<Long, WordNote>): String =
        json.encodeToString(notes.mapValues { (_, n) -> StoredNote(n.text, n.at.toString()) })

    private fun encode(state: StreamState): String = json.encodeToString(StoredState.of(state))

    companion object {
        private const val TAG = "StreamStore"
    }
}

// ------------------------------------------------------------------ JSON shape

@Serializable
private data class StoredNote(val text: String, val at: String)

/**
 * A word's stage as saved. A state saved before the one-pass flow may hold `RELEARNING`
 * stages and `EXPLAIN1` / `EXPLAIN2` steps; [toDomain] reads them as sync.md §2 does.
 */
@Serializable
private data class StoredStage(
    val stage: String = LEARNING,
    val next: String,
    val immediate: Boolean,
    val since: Int,
    val flawed: Boolean = false,
    val attempt: Int = 0,
) {
    /** The stage, or null for a relearning stage: that word stays in review with its card. */
    fun toDomain(): WordStage? {
        if (stage != LEARNING) return null
        val legacy = next in LEGACY_EXPLAIN
        return WordStage(
            next = if (legacy) StepKind.EXPLAIN else StepKind.valueOf(next),
            immediate = immediate && !legacy,
            since = since,
            flawed = flawed,
            attempt = attempt,
        )
    }

    companion object {
        const val LEARNING = "LEARNING"
    }
}

/** Step names saved before the one-pass flow: both are read as a delayed `explain`. */
private val LEGACY_EXPLAIN = setOf("EXPLAIN1", "EXPLAIN2")

private fun stepKind(name: String): StepKind = if (name in LEGACY_EXPLAIN) StepKind.EXPLAIN else StepKind.valueOf(name)

@Serializable
private data class StoredDay(
    val date: String,
    val introduced: Int = 0,
    val extra: Int = 0,
    val steps: Int = 0,
    val reviewed: Int = 0,
    val reviewedClean: Int = 0,
    val met: Int = 0,
    val metClean: Int = 0,
    val cycle: Int = 0,
)

@Serializable
private data class StoredStep(val wordId: Long, val kind: String, val task: String? = null, val attempt: Int = 0)

@Serializable
private data class StoredCard(
    val wordId: Long,
    val due: String,
    val stability: Double,
    val difficulty: Double,
    val elapsedDays: Int = 0,
    val scheduledDays: Int = 0,
    val reps: Int = 0,
    val lapses: Int = 0,
    val state: Int = 0,
    val lastReview: String? = null,
)

@Serializable
private data class StoredLastReview(val wordId: Long, val prev: StoredCard, val grade: String)

@Serializable
private data class StoredState(
    val words: Map<Long, StoredStage> = emptyMap(),
    val seq: Int = 0,
    val recent: List<String> = emptyList(),
    val lastWord: Long? = null,
    val day: StoredDay,
    val current: StoredStep? = null,
    val lastReview: StoredLastReview? = null,
) {
    fun toDomain(): StreamState {
        val stages = words.mapValues { (_, w) -> w.toDomain() }
        val kept = buildMap { stages.forEach { (id, w) -> if (w != null) put(id, w) } }
        // A saved step of an old kind, or of a word whose relearning stage was dropped, is chosen again.
        val step = current?.takeIf { it.kind !in LEGACY_EXPLAIN && !(it.wordId in stages && it.wordId !in kept) }
        return StreamState(
            words = kept,
            seq = seq,
            recent = recent.map(::stepKind),
            lastWord = lastWord,
            day = StreamDay(
                date = LocalDate.parse(day.date),
                introduced = day.introduced,
                extra = day.extra,
                steps = day.steps,
                reviewed = day.reviewed,
                reviewedClean = day.reviewedClean,
                met = day.met,
                metClean = day.metClean,
                cycle = day.cycle,
            ),
            current = step?.let { Step(it.wordId, StepKind.valueOf(it.kind), it.task?.let(ReviewTask::valueOf), it.attempt) },
            lastReview = lastReview?.let { LastReview(it.wordId, it.prev.toDomain(), Grade.valueOf(it.grade)) },
        )
    }

    companion object {
        fun of(s: StreamState) = StoredState(
            words = s.words.mapValues { (_, w) ->
                StoredStage(StoredStage.LEARNING, w.next.name, w.immediate, w.since, w.flawed, w.attempt)
            },
            seq = s.seq,
            recent = s.recent.map { it.name },
            lastWord = s.lastWord,
            day = with(s.day) {
                StoredDay(date.toString(), introduced, extra, steps, reviewed, reviewedClean, met, metClean, cycle)
            },
            current = s.current?.let { StoredStep(it.wordId, it.kind.name, it.task?.name, it.attempt) },
            lastReview = s.lastReview?.let { StoredLastReview(it.wordId, it.prev.stored(), it.grade.name) },
        )
    }
}

private fun StoredCard.toDomain() = FsrsCard(
    wordId = wordId,
    due = Instant.parse(due),
    stability = stability,
    difficulty = difficulty,
    elapsedDays = elapsedDays,
    scheduledDays = scheduledDays,
    reps = reps,
    lapses = lapses,
    state = CardState.fromCode(state),
    lastReview = lastReview?.let(Instant::parse),
)

private fun FsrsCard.stored() = StoredCard(
    wordId = wordId,
    due = due.toString(),
    stability = stability,
    difficulty = difficulty,
    elapsedDays = elapsedDays,
    scheduledDays = scheduledDays,
    reps = reps,
    lapses = lapses,
    state = state.code,
    lastReview = lastReview?.toString(),
)
