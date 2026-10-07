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

/** A word's card once it has graduated, and its stage while it is being learned. */
@Serializable
data class SyncEntry(val card: SyncCard? = null, val stage: SyncStage? = null)

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

/**
 * A stream stage without its stream position, which is per client. An older document's
 * `needClean` and `thenUse` are ignored; its other old forms are read by [normalized].
 */
@Serializable
data class SyncStage(
    /** "learning"; "relearning" only in an older document. */
    val stage: String = LEARNING,
    /** "know", "explain", "spell" or "use"; "explain1" / "explain2" in an older document. */
    val next: String,
    val immediate: Boolean = false,
    val flawed: Boolean = false,
    val attempt: Int = 0,
) {
    /**
     * This stage as the current format reads it (sync.md §2): `explain1` / `explain2` become a
     * delayed `explain`, and a relearning stage is dropped (null), so its word stays in review.
     */
    fun normalized(): SyncStage? = when {
        stage != LEARNING -> null
        next in LEGACY_EXPLAIN -> copy(next = EXPLAIN, immediate = false)
        else -> this
    }

    companion object {
        const val LEARNING = "learning"
        private const val EXPLAIN = "explain"
        private val LEGACY_EXPLAIN = setOf("explain1", "explain2")
    }
}

@Serializable
data class SyncNote(val text: String, val at: String)
