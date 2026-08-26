package dev.morpho.data.repository

import android.util.Log
import dev.morpho.data.db.content.ContentDatabase
import dev.morpho.data.db.content.Distractors
import dev.morpho.data.db.content.Examples
import dev.morpho.data.db.content.Senses
import dev.morpho.data.db.content.Words
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
                row.word_id to WordBundle(
                    word = row.toDomain(),
                    senses = senses[row.word_id].orEmpty().map(Senses::toDomain),
                    examples = examples[row.word_id].orEmpty().map(Examples::toDomain),
                    distractorIds = distractors[row.word_id].orEmpty()
                        .sortedBy(Distractors::rank)
                        .map { it.distractor_word_id },
                )
            }
        }
    }

    /**
     * Loads a question's word plus the three distractor words in a single pass, which
     * is what the quiz screens actually need: the answer bundle and the distractors'
     * image + primary definition.
     */
    suspend fun questionBundles(wordId: Long): QuestionContent? {
        val answer = bundle(wordId) ?: return null
        val optionIds = listOf(wordId) + answer.distractorIds
        val all = bundles(optionIds)
        val missing = optionIds.filterNot { all.containsKey(it) }
        if (missing.isNotEmpty()) {
            Log.e(TAG, "distractor closure violated for word $wordId: missing $missing")
            return null
        }
        return QuestionContent(answer = answer, options = optionIds.mapNotNull { all[it] })
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
            val orders = db.wordsQueries.selectPlanSlice().executeAsList()
            if (orders.map { it.learning_order }.toSet().size != orders.size) {
                add("learning_order is not unique across the release")
            }
        }
    }

    /** Every media filename the release references — used by the demo media generator. */
    suspend fun allMediaFiles(): MediaManifest = withContext(Dispatchers.IO) {
        val words = db.wordsQueries.selectAllOrdered().executeAsList()
        val images = words.mapTo(LinkedHashSet()) { it.image_file }
        val audio = LinkedHashSet<String>()
        words.forEach { audio += it.word_audio_file }
        db.sensesQueries.selectForWords(words.map { it.word_id }).executeAsList()
            .forEach { audio += it.def_audio_file }
        db.examplesQueries.selectForWords(words.map { it.word_id }).executeAsList()
            .forEach { audio += it.ex_audio_file }
        MediaManifest(images = images, audio = audio)
    }

    fun invalidate() {
        cachedPlan = null
    }

    companion object {
        private const val TAG = "ContentRepository"
    }
}

data class QuestionContent(
    val answer: WordBundle,
    /** The answer first, then its three distractors in rank order. */
    val options: List<WordBundle>,
)

data class MediaManifest(
    val images: Set<String>,
    val audio: Set<String>,
)

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
    imageFile = image_file,
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
)
