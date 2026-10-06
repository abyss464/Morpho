package dev.morpho.data.repository

import dev.morpho.data.db.content.ContentDatabase
import dev.morpho.data.db.content.Distractors
import dev.morpho.data.db.content.Examples
import dev.morpho.data.db.content.Senses
import dev.morpho.data.db.content.Words
import dev.morpho.domain.content.GlossAnchor
import dev.morpho.domain.content.GlossIndex
import dev.morpho.domain.model.ContentMetaKeys
import dev.morpho.domain.model.Example
import dev.morpho.domain.model.GroupType
import dev.morpho.domain.model.PlanWord
import dev.morpho.domain.model.Sense
import dev.morpho.domain.model.Word
import dev.morpho.domain.model.WordBundle
import dev.morpho.domain.model.WordGroup
import dev.morpho.domain.model.WordRole
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/**
 * Read-only access to release.db, mapped into the domain model.
 *
 * Everything here is a plain suspending read on [Dispatchers.IO]: the content DB never
 * changes while the app runs, so there is nothing to observe and no cache invalidation
 * to get wrong. The plan and the shipped-id set are memoised because every session
 * build needs them.
 */
class ContentRepository(private val db: ContentDatabase) {

    @Volatile
    private var cachedPlan: List<PlanWord>? = null

    @Volatile
    private var cachedGlossIndex: GlossIndex? = null

    suspend fun planWords(): List<PlanWord> = cachedPlan ?: withContext(Dispatchers.IO) {
        db.wordsQueries.selectPlanSlice().executeAsList().map {
            PlanWord(
                wordId = it.word_id,
                groupId = it.group_id,
                learningOrder = it.learning_order.toInt(),
            )
        }.also { cachedPlan = it }
    }

    suspend fun shippedWordIds(): Set<Long> = planWords().mapTo(HashSet()) { it.wordId }

    suspend fun wordCount(): Int = withContext(Dispatchers.IO) {
        db.wordsQueries.countAll().executeAsOne().toInt()
    }

    /**
     * The whole `gloss_anchors` table as a lookup index.
     *
     * A few hundred rows against which every definition and sentence on screen is
     * scanned, so it is read once and held for the life of the process — the content DB
     * is immutable, and re-querying per definition would be a query per frame.
     */
    suspend fun glossIndex(): GlossIndex = cachedGlossIndex ?: withContext(Dispatchers.IO) {
        GlossIndex.of(
            db.glossAnchorsQueries.selectAll().executeAsList().map {
                GlossAnchor(wordId = it.word_id, word = it.word, zhGloss = it.zh_gloss)
            },
        ).also { cachedGlossIndex = it }
    }

    suspend fun isEmpty(): Boolean = wordCount() == 0

    suspend fun contentVersion(): String? = metaValue(ContentMetaKeys.CONTENT_VERSION)

    suspend fun metaValue(key: String): String? = withContext(Dispatchers.IO) {
        db.metaQueries.selectValue(key).executeAsOneOrNull()
    }

    suspend fun group(groupId: Long): WordGroup? = withContext(Dispatchers.IO) {
        db.groupsQueries.selectById(groupId).executeAsOneOrNull()?.let {
            WordGroup(it.group_id, it.group_order.toInt(), GroupType.fromDb(it.group_type))
        }
    }

    suspend fun word(wordId: Long): Word? = withContext(Dispatchers.IO) {
        db.wordsQueries.selectById(wordId).executeAsOneOrNull()?.toDomain()
    }

    suspend fun words(wordIds: Collection<Long>): List<Word> {
        if (wordIds.isEmpty()) return emptyList()
        return withContext(Dispatchers.IO) {
            db.wordsQueries.selectByIds(wordIds).executeAsList().map { it.toDomain() }
        }
    }

    /**
     * Loads a word together with its senses, examples and the three bound distractors.
     *
     * The release build is dependency-closed over distractor edges, so the caller can
     * rely on every distractor id resolving to a fully provisioned word.
     */
    suspend fun bundle(wordId: Long): WordBundle? = bundles(listOf(wordId))[wordId]

    /** Batched form: one query per table for the whole question, not per word. */
    suspend fun bundles(wordIds: Collection<Long>): Map<Long, WordBundle> {
        if (wordIds.isEmpty()) return emptyMap()
        val ids = wordIds.distinct()
        return withContext(Dispatchers.IO) {
            val words = db.wordsQueries.selectByIds(ids).executeAsList()
            val senses = db.sensesQueries.selectForWords(ids).executeAsList().groupBy { it.word_id }
            val examples = db.examplesQueries.selectForWords(ids).executeAsList().groupBy { it.word_id }
            val distractors = db.distractorsQueries.selectForWords(ids).executeAsList()
                .groupBy { it.word_id }

            words.associate { row ->
                val wordExamples = examples[row.word_id].orEmpty().map(Examples::toDomain)
                val cardImageFile = wordExamples.minByOrNull { it.displayOrder }?.imageFile.orEmpty()
                row.word_id to WordBundle(
                    word = row.toDomain().copy(imageFile = cardImageFile),
                    senses = senses[row.word_id].orEmpty().map(Senses::toDomain),
                    examples = wordExamples,
                    distractorIds = distractors[row.word_id].orEmpty()
                        .sortedBy(Distractors::rank)
                        .map { it.distractor_word_id },
                )
            }
        }
    }

    /**
     * Debug-build assertion scan (README Part 6: "只在 debug 构建启动时跑一次全量断言扫描").
     * Returns a list of human-readable violations; empty means the release is sound.
     */
    suspend fun assertIntegrity(): List<String> = withContext(Dispatchers.IO) {
        buildList {
            db.sensesQueries.selectWordsWithBadPrimaryCount().executeAsList().forEach {
                add("word ${it.word_id} has ${it.primary_count} primary senses, expected exactly 1")
            }
            db.distractorsQueries.selectWordsWithBadDistractorCount().executeAsList().forEach {
                add("word ${it.word_id} has ${it.bound} bound distractors, expected 3")
            }
            db.distractorsQueries.selectDanglingDistractors().executeAsList().forEach {
                add("word ${it.word_id} rank ${it.rank} points at unshipped word ${it.distractor_word_id}")
            }
            db.examplesQueries.selectWordsWithoutFirstExample().executeAsList().forEach {
                add("word $it has no display_order 1 example, so its card and use step have no sentence")
            }
            db.examplesQueries.selectExamplesWithBadHighlight().executeAsList().forEach {
                add("example ${it.example_id} (word ${it.word_id}) has an out-of-range highlight")
            }
            db.glossAnchorsQueries.selectAnchorsShadowingWords().executeAsList().forEach {
                add("gloss anchor ${it.word_id} '${it.word}' shadows a shipped word")
            }
            val orders = db.wordsQueries.selectPlanSlice().executeAsList()
            if (orders.map { it.learning_order }.toSet().size != orders.size) {
                add("learning_order is not unique across the release")
            }
        }
    }
}

// ------------------------------------------------------------------ mapping

private fun Words.toDomain() = Word(
    wordId = word_id,
    word = word,
    phonetic = phonetic,
    frequencyRank = frequency_rank?.toInt(),
    role = WordRole.fromDb(role),
    groupId = group_id,
    learningOrder = learning_order.toInt(),
    etymology = etymology,
    etymologySegmentsJson = etymology_segments,
    imageFile = "",
    wordAudioFile = word_audio_file,
)

private fun Senses.toDomain() = Sense(
    senseId = sense_id,
    wordId = word_id,
    pos = pos,
    definition = definition,
    isPrimary = is_primary != 0L,
    defAudioFile = def_audio_file,
)

private fun Examples.toDomain() = Example(
    exampleId = example_id,
    wordId = word_id,
    displayOrder = display_order.toInt(),
    sentence = sentence,
    hlStart = hl_start.toInt(),
    hlEnd = hl_end.toInt(),
    exAudioFile = ex_audio_file,
    imageFile = image_file,
)
