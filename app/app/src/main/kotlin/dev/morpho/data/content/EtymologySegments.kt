package dev.morpho.data.content

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonPrimitive

/**
 * Codec for `words.etymology_segments` (docs/contracts/release-db.sql): a JSON array of
 * morph segments such as `["bene","vol","ent"]`, nullable.
 *
 * This column is the **only** structured source for `EtymologyChips`. When it is null or
 * unparseable the sheet falls back to prose alone — a release is free to ship an
 * etymology it has no agreed segmentation for.
 *
 * Deliberately free of Android types, so its JVM unit test exercises the same code the
 * app runs rather than a stand-in.
 */
object EtymologySegments {

    private val json = Json { ignoreUnknownKeys = true }

    /**
     * Decodes the column value. Returns an empty list for null, blank, malformed, or
     * non-array input: a bad row degrades to prose-only rather than breaking the sheet.
     */
    fun parse(raw: String?): List<String> {
        if (raw.isNullOrBlank()) return emptyList()
        val array = runCatching { json.parseToJsonElement(raw) }.getOrNull() as? JsonArray
            ?: return emptyList()
        return array.mapNotNull { item ->
            (item as? JsonPrimitive)?.takeIf { it.isString }?.content?.trim()?.ifBlank { null }
        }
    }

    /**
     * Inverse of [parse]. The app never writes release.db, so this exists to pin the
     * column's shape from the consuming side: a round-trip test here is what would
     * catch the exporter and the reader drifting apart on encoding.
     */
    fun encode(segments: List<String>): String? {
        val cleaned = segments.map { it.trim() }.filter { it.isNotEmpty() }
        if (cleaned.isEmpty()) return null
        return JsonArray(cleaned.map(::JsonPrimitive)).toString()
    }
}
