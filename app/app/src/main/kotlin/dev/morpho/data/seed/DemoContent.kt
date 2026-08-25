package dev.morpho.data.seed

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/**
 * Shape of `assets/demo/demo_content.json` — the wave-1 stand-in for a real
 * `release.db` produced by `morphod export`.
 *
 * Deliberately more forgiving than the release schema: distractors and highlight
 * spans are written as plain words, and [DemoContentSeeder] resolves them into
 * word ids and UTF-8 byte offsets. That keeps the hand-written fixture readable and
 * makes it impossible to write an inconsistent id by hand.
 */
@Serializable
data class DemoContent(
    @SerialName("content_version") val contentVersion: String,
    @SerialName("plan_id") val planId: String,
    @SerialName("schema_ver") val schemaVer: String,
    val note: String? = null,
    val groups: List<DemoGroup>,
    val words: List<DemoWord>,
)

@Serializable
data class DemoGroup(
    @SerialName("group_id") val groupId: Long,
    @SerialName("group_order") val groupOrder: Int,
    @SerialName("group_type") val groupType: String,
)

@Serializable
data class DemoWord(
    val word: String,
    val phonetic: String? = null,
    @SerialName("frequency_rank") val frequencyRank: Int? = null,
    val role: String = "target",
    @SerialName("group_id") val groupId: Long,
    val etymology: String? = null,
    @SerialName("etymology_segments") val etymologySegments: List<String> = emptyList(),
    val senses: List<DemoSense>,
    val examples: List<DemoExample>,
    /** Exactly three, each naming another word in this file. */
    val distractors: List<String>,
)

@Serializable
data class DemoSense(
    val pos: String,
    val definition: String,
    @SerialName("is_primary") val isPrimary: Boolean = false,
)

@Serializable
data class DemoExample(
    val sentence: String,
    /** Substring of [sentence] to highlight; resolved to byte offsets at seed time. */
    val highlight: String,
)
