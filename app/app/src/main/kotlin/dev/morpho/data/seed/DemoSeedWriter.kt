package dev.morpho.data.seed

import dev.morpho.data.content.EtymologySegments
import dev.morpho.data.db.content.ContentDatabase
import dev.morpho.domain.model.ContentMetaKeys

/**
 * Writes a parsed [DemoContent] fixture into a release-shaped database.
 *
 * Split out of [DemoContentSeeder] so it has no Android dependency: a plain JVM unit
 * test can create the real SQLDelight schema over an in-memory SQLite database, run
 * this writer against the shipped JSON, and assert that the fixture satisfies every
 * constraint the release contract imposes.
 */
object DemoSeedWriter {

    /**
     * Demo word ids start well above zero so a later real release, whose ids come from
     * the working DB, cannot silently collide with demo progress rows.
     */
    const val BASE_WORD_ID = 900_001L

    const val IMAGE_DIR = "img"
    const val AUDIO_DIR = "audio"

    /**
     * Extensions differ from the release contract on purpose: the on-device demo
     * generator can only synthesise PNG and WAV. `ContentStore` resolves names
     * verbatim, and neither Coil nor Media3 keys off the extension.
     */
    const val IMAGE_EXT = "png"
    const val AUDIO_EXT = "wav"

    fun seed(db: ContentDatabase, content: DemoContent): Int {
        val wordIds = content.words
            .mapIndexed { index, word -> word.word.lowercase() to BASE_WORD_ID + index }
            .toMap()

        validate(content, wordIds)

        db.transaction {
            content.groups.forEach {
                db.groupsQueries.insert(it.groupId, it.groupOrder.toLong(), it.groupType)
            }

            // Pass 1: every word row. Distractors point at other words, so they can only
            // be written once the whole word set exists (foreign keys are enforced).
            content.words.forEachIndexed { index, word ->
                db.wordsQueries.insert(
                    word_id = wordIds.getValue(word.word.lowercase()),
                    word = word.word,
                    phonetic = word.phonetic,
                    frequency_rank = word.frequencyRank?.toLong(),
                    role = word.role,
                    group_id = word.groupId,
                    learning_order = (index + 1).toLong(),
                    etymology = word.etymology,
                    etymology_segments = EtymologySegments.encode(word.etymologySegments),
                    image_file = mediaName(IMAGE_DIR, "${word.word}-image", IMAGE_EXT),
                    word_audio_file = mediaName(AUDIO_DIR, "${word.word}-word", AUDIO_EXT),
                )
            }

            // Pass 2: everything that references a word.
            var senseId = 1L
            var exampleId = 1L
            content.words.forEach { word ->
                val wordId = wordIds.getValue(word.word.lowercase())

                word.senses.forEach { sense ->
                    db.sensesQueries.insert(
                        sense_id = senseId,
                        word_id = wordId,
                        pos = sense.pos,
                        definition = sense.definition,
                        is_primary = if (sense.isPrimary) 1L else 0L,
                        def_audio_file = mediaName(
                            AUDIO_DIR,
                            "${word.word}-def-${sense.pos}-${sense.definition.length}-$senseId",
                            AUDIO_EXT,
                        ),
                    )
                    senseId++
                }

                word.examples.forEachIndexed { exIndex, example ->
                    val (start, end) = byteHighlight(example.sentence, example.highlight)
                    db.examplesQueries.insert(
                        example_id = exampleId,
                        word_id = wordId,
                        display_order = (exIndex + 1).toLong(),
                        sentence = example.sentence,
                        hl_start = start.toLong(),
                        hl_end = end.toLong(),
                        ex_audio_file = mediaName(
                            AUDIO_DIR,
                            "${word.word}-ex-${exIndex + 1}",
                            AUDIO_EXT,
                        ),
                    )
                    exampleId++
                }

                word.distractors.forEachIndexed { rank, lemma ->
                    db.distractorsQueries.insert(
                        word_id = wordId,
                        rank = (rank + 1).toLong(),
                        distractor_word_id = wordIds.getValue(lemma.lowercase()),
                    )
                }
            }

            db.metaQueries.upsert(ContentMetaKeys.CONTENT_VERSION, content.contentVersion)
            db.metaQueries.upsert(ContentMetaKeys.PLAN_ID, content.planId)
            db.metaQueries.upsert(ContentMetaKeys.SCHEMA_VER, content.schemaVer)
            db.metaQueries.upsert(ContentMetaKeys.EXPORTED_AT, "1970-01-01T00:00:00Z")
        }
        return content.words.size
    }

    /** Fails loudly on a malformed fixture rather than shipping a broken demo. */
    fun validate(content: DemoContent, wordIds: Map<String, Long>) {
        val groupIds = content.groups.mapTo(HashSet()) { it.groupId }
        content.words.forEach { word ->
            require(word.groupId in groupIds) { "${word.word}: unknown group ${word.groupId}" }
            require(word.senses.count { it.isPrimary } == 1) {
                "${word.word}: needs exactly one primary sense"
            }
            require(word.examples.isNotEmpty()) { "${word.word}: needs a mode-1 example" }
            require(word.distractors.size == 3) { "${word.word}: needs exactly 3 distractors" }
            require(word.distractors.distinct().size == 3) { "${word.word}: duplicate distractors" }
            word.distractors.forEach { lemma ->
                require(wordIds.containsKey(lemma.lowercase())) {
                    "${word.word}: distractor '$lemma' is not in the release"
                }
                require(!lemma.equals(word.word, ignoreCase = true)) {
                    "${word.word}: cannot be its own distractor"
                }
            }
            word.examples.forEach { example ->
                require(example.sentence.contains(example.highlight, ignoreCase = true)) {
                    "${word.word}: highlight '${example.highlight}' is not in the sentence"
                }
            }
        }
    }

    /** UTF-8 byte offsets, as the release contract specifies. */
    fun byteHighlight(sentence: String, target: String): Pair<Int, Int> {
        val charStart = sentence.indexOf(target, ignoreCase = true)
        if (charStart < 0) return 0 to 0
        val start = sentence.substring(0, charStart).toByteArray(Charsets.UTF_8).size
        val end = start + target.toByteArray(Charsets.UTF_8).size
        return start to end
    }

    /** Mimics content addressing: a stable hex digest of the logical content. */
    fun mediaName(dir: String, key: String, ext: String): String {
        var hash = -0x340d631b7bdddcdbL
        for (ch in key) {
            hash = hash xor ch.code.toLong()
            hash *= 0x100000001B3L
        }
        val hex = java.lang.Long.toHexString(hash).padStart(16, '0')
        return "$dir/$hex.$ext"
    }
}
