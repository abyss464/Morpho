package dev.morpho.data.db

import app.cash.sqldelight.db.QueryResult
import app.cash.sqldelight.db.SqlCursor
import app.cash.sqldelight.driver.jdbc.sqlite.JdbcSqliteDriver
import dev.morpho.data.db.content.ContentDatabase
import dev.morpho.data.repository.ContentRepository
import dev.morpho.domain.learning.LearningEngine
import dev.morpho.domain.learning.OptionAssembler
import dev.morpho.domain.learning.SessionConfig
import dev.morpho.domain.model.ContentMetaKeys
import dev.morpho.domain.model.LearnMode
import java.io.File
import kotlinx.coroutines.runBlocking
import kotlin.test.AfterTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNotNull
import kotlin.test.assertTrue

/**
 * Opens the **real** `release.db` that ships in `assets/` and runs the app's own queries
 * against it, on the JVM, with no device.
 *
 * This replaces the wave-1 demo-fixture test and does strictly more: back then the point
 * was that the `.sq` DDL was valid SQLite and that an invented fixture satisfied it. Now
 * the DDL has to match a file produced by a different program, in a different language,
 * from a different repository — so the assertion that matters is that every generated
 * query compiles *and executes* against the bytes actually in the APK. A column renamed
 * on either side of the contract fails here rather than on the user's phone.
 */
class ReleaseDatabaseTest {

    private val driver = JdbcSqliteDriver("jdbc:sqlite:${RELEASE_DB.absolutePath}")
    private val db = ContentDatabase(driver)
    private val repo = ContentRepository(db)

    @AfterTest
    fun tearDown() = driver.close()

    // ------------------------------------------------------------------ shape

    @Test
    fun `the bundled release carries the counts the export reported`() {
        assertTrue(RELEASE_DB.isFile, "no bundled release at ${RELEASE_DB.absolutePath}")
        assertEquals(4_225L, db.wordsQueries.countAll().executeAsOne(), "words")
        assertEquals(5_067L, db.sensesQueries.countAll().executeAsOne(), "senses")
        assertEquals(12_667L, db.examplesQueries.countAll().executeAsOne(), "examples")
        assertEquals(12_675L, db.distractorsQueries.countAll().executeAsOne(), "distractors")
        assertEquals(451L, db.glossAnchorsQueries.countAll().executeAsOne(), "gloss anchors")
        assertEquals(234L, db.groupsQueries.countAll().executeAsOne(), "groups")
    }

    @Test
    fun `meta identifies the release the app was built against`() {
        assertEquals(
            "2026.09.01+44c6a2f7",
            db.metaQueries.selectValue(ContentMetaKeys.CONTENT_VERSION).executeAsOneOrNull(),
        )
        assertNotNull(db.metaQueries.selectValue(ContentMetaKeys.PLAN_ID).executeAsOneOrNull())
        assertNotNull(db.metaQueries.selectValue(ContentMetaKeys.SCHEMA_VER).executeAsOneOrNull())
        // A DATE, per the wave-3 rulings — not a timestamp.
        val exportedAt = db.metaQueries.selectValue(ContentMetaKeys.EXPORTED_AT).executeAsOneOrNull()
        assertNotNull(exportedAt)
        assertTrue(exportedAt.matches(Regex("""\d{4}-\d{2}-\d{2}""")), "exported_at=$exportedAt")
    }

    /**
     * The export leaves `user_version` at 0, which the Android open helper would read as
     * an empty file and answer by running `CREATE TABLE` over populated tables.
     * `DatabaseProvider.stampUserVersion` is what stops that, so pin the premise: if a
     * future export starts stamping the schema version, this fails and the restamp can
     * be revisited deliberately rather than discovered in a crash log.
     */
    @Test
    fun `the shipped file needs the install-time user_version restamp`() {
        val stamped = query("PRAGMA user_version") { it.getLong(0) }.single()
        assertEquals(0L, stamped, "export started stamping user_version; revisit stampUserVersion")
    }

    // -------------------------------------------------------------- integrity

    @Test
    fun `the release satisfies every assertion the debug build scans for`() = runBlocking {
        assertEquals(emptyList(), repo.assertIntegrity())
    }

    @Test
    fun `learning order is dense and unique across all four thousand words`() = runBlocking {
        val plan = repo.planWords()
        assertEquals(4_225, plan.size)
        assertEquals((1..4_225).toList(), plan.map { it.learningOrder })
    }

    @Test
    fun `every word has a mode-1 sentence and every highlight is in range`() {
        assertTrue(
            db.examplesQueries.selectWordsWithoutFirstExample().executeAsList().isEmpty(),
        )
        assertTrue(
            db.examplesQueries.selectExamplesWithBadHighlight().executeAsList().isEmpty(),
        )
    }

    /**
     * The SQL range check above proves the offsets *fit* the sentence; this proves they
     * point at the right thing. Decoding all 12,377 sentences is too slow to run at app
     * startup and exactly the right cost for a build.
     *
     * The shipped sentences inflect — "abandon" is highlighted as `abandoned`, "dine"
     * as `dining` — so demanding the lemma verbatim would fail on 2,105 perfectly good
     * examples. What must hold instead, and what an off-by-one byte offset would break
     * immediately, is that the span covers a **whole token** which **starts with the
     * word's stem**. That catches a mis-encoded offset without pretending English has no
     * morphology, which would be a strange thing for this app to assume.
     */
    @Test
    fun `highlights cover a whole inflected token of their own word`() {
        val words = query("SELECT word_id, word FROM words") { it.getLong(0)!! to it.getString(1)!! }
            .toMap()
        var checked = 0
        var inflected = 0
        query("SELECT example_id, word_id, sentence, hl_start, hl_end FROM examples") {
            Highlight(
                exampleId = it.getLong(0)!!,
                wordId = it.getLong(1)!!,
                sentence = it.getString(2)!!,
                start = it.getLong(3)!!.toInt(),
                end = it.getLong(4)!!.toInt(),
            )
        }.forEach { row ->
            val bytes = row.sentence.toByteArray(Charsets.UTF_8)
            val before = String(bytes, 0, row.start, Charsets.UTF_8)
            val covered = String(bytes, row.start, row.end - row.start, Charsets.UTF_8)
            val after = String(bytes, row.end, bytes.size - row.end, Charsets.UTF_8)

            assertTrue(
                before.lastOrNull()?.isLetter() != true && after.firstOrNull()?.isLetter() != true,
                "example ${row.exampleId} cuts a word in half at '$covered'",
            )

            val word = words.getValue(row.wordId).lowercase()
            // Silent-e verbs drop it before a vowel suffix: dine -> dining.
            val stem = word.removeSuffix("e")
            assertTrue(
                covered.lowercase().startsWith(stem),
                "example ${row.exampleId} highlights '$covered' for the word '$word'",
            )
            if (!covered.equals(word, ignoreCase = true)) inflected++
            checked++
        }
        assertEquals(12_667, checked)
        assertTrue(inflected > 0, "no inflected highlight at all looks like a fixture, not a release")
    }

    // ------------------------------------------------------------ gloss anchors

    @Test
    fun `gloss anchors resolve inside the definitions that reference them`() = runBlocking {
        val index = repo.glossIndex()
        assertEquals(451, index.size)

        // An index that never fires on shipped text is an index that silently does
        // nothing, so pin that it actually lights words up across the release.
        val definitions = query("SELECT definition FROM senses") { it.getString(0)!! }
        val hits = definitions.count { index.scan(it).isNotEmpty() }
        assertTrue(hits > 100, "only $hits of ${definitions.size} definitions matched an anchor")

        // Whole-word matching, not substring: no anchor may fire on a word that merely
        // contains it. Checked against the shipped lemmas themselves, which are the
        // strings most likely to nest inside one another.
        assertTrue(index.scan("favourites").isEmpty(), "'rites' leaked out of 'favourites'")
    }

    // ---------------------------------------------------------- session at scale

    @Test
    fun `a full day's session builds and every question resolves its four options`() = runBlocking {
        val config = SessionConfig(sessionSeed = 20_260_826L)
        val plan = LearningEngine.buildSession(
            plan = repo.planWords(),
            progress = emptyMap(),
            dueReviewIds = emptyList(),
            newWordQuota = config.dailyGoal,
            config = config,
        )
        assertTrue(plan.units.size >= 2, "a 50-word day should cut into more than one unit")
        assertEquals(config.dailyGoal, plan.newWordCount)

        var state = LearningEngine.startSession(plan, emptyMap(), config)
        var questions = 0
        val modesSeen = mutableMapOf<Long, MutableSet<LearnMode>>()

        while (!state.finished && state.currentQuestion != null) {
            val question = state.currentQuestion!!
            modesSeen.getOrPut(question.wordId) { mutableSetOf() } += question.mode

            val content = repo.questionBundles(question.wordId)
            assertNotNull(content, "no content for word ${question.wordId}")
            assertEquals(4, content.options.size)
            assertEquals(
                4,
                OptionAssembler.assemble(
                    answerWordId = question.wordId,
                    distractorIds = content.answer.distractorIds,
                    seed = question.optionSeed(config.sessionSeed),
                ).toSet().size,
                "duplicate option ids on word ${question.wordId}",
            )
            if (question.mode == LearnMode.SENTENCE_IMAGE) {
                assertNotNull(content.answer.mode1Example, "mode 1 needs a sentence")
            }

            state = LearningEngine.submitAnswer(state, correct = true).state
            questions++
            assertTrue(questions < 1_000, "session did not terminate")
        }

        assertEquals(config.dailyGoal, state.learnedThisSession.size)
        assertEquals(config.dailyGoal * config.roundsRequired, questions)
        assertTrue(modesSeen.values.all { it.size == 3 }, "some words never reached mode 3")
    }

    /**
     * The detail sheet is the one screen that renders *everything* a word has, and the
     * real release has words with four senses where the demo had one. Load the widest
     * ones and check the sheet's inputs are all there — in particular a per-sense audio
     * clip, since the sheet gives every sense its own play button.
     */
    @Test
    fun `the widest multi-sense words carry a distinct audio clip per sense`() = runBlocking {
        val widest = query(
            "SELECT word_id, count(*) c FROM senses GROUP BY word_id ORDER BY c DESC LIMIT 12",
        ) { it.getLong(0)!! to it.getLong(1)!! }
        assertTrue(widest.isNotEmpty())
        assertTrue(widest.first().second >= 3, "expected a word with 3+ senses")

        widest.forEach { (wordId, senseCount) ->
            val bundle = repo.bundle(wordId)
            assertNotNull(bundle, "no bundle for word $wordId")
            assertEquals(senseCount, bundle.senses.size.toLong())
            assertTrue(bundle.senses.first().isPrimary, "primary sense must sort first")
            assertEquals(
                bundle.senses.size,
                bundle.senses.map { it.defAudioFile }.toSet().size,
                "word $wordId reuses a definition clip across senses",
            )
            bundle.senses.forEach { assertTrue(it.defAudioFile.startsWith("audio/")) }
            assertTrue(bundle.examples.isNotEmpty())
        }
    }

    @Test
    fun `media filenames follow the normative bundle layout`() {
        db.wordsQueries.selectAllOrdered().executeAsList().forEach {
            assertTrue(
                it.word_audio_file.startsWith("audio/") && it.word_audio_file.endsWith(".ogg"),
            )
        }
        query(
            "SELECT image_file FROM examples WHERE display_order = 1 AND image_file IS NOT NULL",
        ) { it.getString(0)!! }.forEach {
            assertTrue(it.startsWith("img/") && it.endsWith(".webp"))
        }
    }

    // ------------------------------------------------------------------ helpers

    private data class Highlight(
        val exampleId: Long,
        val wordId: Long,
        val sentence: String,
        val start: Int,
        val end: Int,
    )

    /**
     * Ad-hoc read for assertions the app itself has no reason to make. Deliberately raw
     * SQL rather than a generated query: `.sq` files mirror the release contract, and
     * padding them with test-only statements would blur what the app actually runs.
     */
    private fun <T> query(sql: String, row: (SqlCursor) -> T): List<T> =
        driver.executeQuery(
            identifier = null,
            sql = sql,
            mapper = { cursor ->
                val out = ArrayList<T>()
                while (cursor.next().value) out += row(cursor)
                QueryResult.Value(out)
            },
            parameters = 0,
        ).value

    private companion object {
        /** Unit tests run with the module directory as their working directory. */
        val RELEASE_DB = File("src/main/assets/release.db")
    }
}
