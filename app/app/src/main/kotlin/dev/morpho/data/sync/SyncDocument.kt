package dev.morpho.data.sync

import kotlinx.serialization.Serializable

/**
 * The progress document the web app and this app sync through (docs/contracts/sync.md §2).
 * Words and notes are keyed by word id as a string, as JSON object keys are.
 */
@Serializable
data class SyncDocument(
    val v: Int = VERSION,
    val words: Map<String, SyncEntry> = emptyMap(),
    val notes: Map<String, SyncNote> = emptyMap(),
) {
    companion object {
        const val VERSION = 1
    }
}

/** A word's card once it has graduated, and its stage while it is being learned or relearned. */
@Serializable
data class SyncEntry(val card: SyncCard? = null, val stage: SyncStage? = null)

/** An FSRS card, field for field; times are ISO-8601 UTC, state 0 New … 3 Relearning. */
@Serializable
data class SyncCard(
    val due: String,
    val stability: Double,
    val difficulty: Double,
    val elapsedDays: Int,
    val scheduledDays: Int,
    val reps: Int,
    val lapses: Int,
    val state: Int,
    val lastReview: String? = null,
)

/** A stream stage without its stream position, which is per client. */
@Serializable
data class SyncStage(
    /** "learning" or "relearning". */
    val stage: String,
    /** "know", "explain1", "explain2" or "use". */
    val next: String,
    val immediate: Boolean,
    val needClean: Int,
    val thenUse: Boolean,
    val flawed: Boolean,
)

@Serializable
data class SyncNote(val text: String, val at: String)
