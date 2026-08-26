package dev.morpho.domain.model

/**
 * Read-only content model. Mirrors `docs/contracts/release-db.sql` one-for-one.
 *
 * Everything here is produced by `morphod export` and shipped inside the APK; the
 * app never mutates it. Media is referenced by content-addressed filename
 * (`img/{hash}.webp`, `audio/{hash}.ogg`) which a `ContentStore` resolves to bytes.
 */

enum class WordRole {
    TARGET,
    AUXILIARY,
    ;

    val dbValue: String get() = if (this == TARGET) "target" else "auxiliary"

    companion object {
        fun fromDb(value: String): WordRole = when (value) {
            "target" -> TARGET
            "auxiliary" -> AUXILIARY
            else -> error("unknown word role: $value")
        }
    }
}

enum class GroupType {
    SCC,
    ROOT,
    SEMANTIC,
    FILL,
    ;

    val dbValue: String get() = name.lowercase()

    companion object {
        fun fromDb(value: String): GroupType = when (value) {
            "scc" -> SCC
            "root" -> ROOT
            "semantic" -> SEMANTIC
            "fill" -> FILL
            else -> error("unknown group type: $value")
        }
    }
}

data class Word(
    val wordId: Long,
    val word: String,
    val phonetic: String?,
    val frequencyRank: Int?,
    val role: WordRole,
    val groupId: Long,
    val learningOrder: Int,
    val etymology: String?,
    /**
     * `words.etymology_segments` verbatim — a JSON array of morph segments such as
     * `["bene","vol","ent"]`, or null when the release has no segmentation for this
     * word. Kept as the raw column value so the domain stays free of a JSON parser;
     * the presentation layer decodes it for `EtymologyChips`.
     */
    val etymologySegmentsJson: String?,
    val imageFile: String,
    val wordAudioFile: String,
)

data class Sense(
    val senseId: Long,
    val wordId: Long,
    val pos: String,
    val definition: String,
    val isPrimary: Boolean,
    val defAudioFile: String,
)

/**
 * [hlStart] / [hlEnd] are **UTF-8 byte offsets** into [sentence] as specified by the
 * release contract. Use [highlightCharRange] to convert them for Kotlin string slicing.
 */
data class Example(
    val exampleId: Long,
    val wordId: Long,
    val displayOrder: Int,
    val sentence: String,
    val hlStart: Int,
    val hlEnd: Int,
    val exAudioFile: String,
) {
    /** Converts the contract's UTF-8 byte offsets into a Kotlin char range. */
    fun highlightCharRange(): IntRange {
        val bytes = sentence.toByteArray(Charsets.UTF_8)
        if (hlStart < 0 || hlEnd > bytes.size || hlStart >= hlEnd) return IntRange.EMPTY
        val startChars = String(bytes, 0, hlStart, Charsets.UTF_8).length
        val endChars = String(bytes, 0, hlEnd, Charsets.UTF_8).length
        return startChars until endChars
    }
}

data class WordGroup(
    val groupId: Long,
    val groupOrder: Int,
    val groupType: GroupType,
)

/**
 * A word with everything a quiz question or the detail sheet needs, loaded in one go.
 *
 * The release build is dependency-closed over distractor edges, so [distractorIds]
 * always resolves to three fully-provisioned shipped words — the app carries no
 * fallback path (README Part 6, "distractor guarantee").
 */
data class WordBundle(
    val word: Word,
    val senses: List<Sense>,
    val examples: List<Example>,
    val distractorIds: List<Long>,
) {
    val primarySense: Sense
        get() = senses.firstOrNull { it.isPrimary }
            ?: senses.firstOrNull()
            ?: error("word ${word.wordId} has no senses")

    /** display_order 1 is the mode-1 sentence; 2-3 live on the detail sheet. */
    val mode1Example: Example?
        get() = examples.minByOrNull { it.displayOrder }

    val detailExamples: List<Example>
        get() = examples.sortedBy { it.displayOrder }
}

/** Minimal per-word plan row, used by session building without loading full bundles. */
data class PlanWord(
    val wordId: Long,
    val groupId: Long,
    val learningOrder: Int,
)

/** `meta` keys defined by the release contract. */
object ContentMetaKeys {
    const val CONTENT_VERSION = "content_version"
    const val PLAN_ID = "plan_id"
    const val EXPORTED_AT = "exported_at"
    const val SCHEMA_VER = "schema_ver"
}
