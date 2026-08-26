package dev.morpho.data.seed

import app.cash.sqldelight.driver.jdbc.sqlite.JdbcSqliteDriver
import dev.morpho.data.content.EtymologySegments
import dev.morpho.data.db.content.ContentDatabase
import dev.morpho.domain.model.ContentMetaKeys
import java.io.File
import kotlinx.serialization.json.Json
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNotNull
import kotlin.test.assertNull
import kotlin.test.assertTrue

/**
 * Builds the real release schema in memory and seeds it with the shipped demo JSON.
 *
 * This is the wave-1 substitute for `morphod`'s export validator: it proves the `.sq`
 * DDL is valid SQLite, that the fixture satisfies every constraint the release
 * contract imposes, and that the app's integrity assertions pass on it.
 */
class DemoFixtureTest {

    private fun loadFixture(): DemoContent {
        val file = File("src/main/assets/${DemoContentSeeder.ASSET}")
        assertTrue(file.exists(), "demo fixture missing at ${file.absolutePath}")
        return Json { ignoreUnknownKeys = true }.decodeFromString(file.readText())
    }

    private fun seededDatabase(): Pair<ContentDatabase, DemoContent> {
        val driver = JdbcSqliteDriver(JdbcSqliteDriver.IN_MEMORY)
        ContentDatabase.Schema.create(driver)
        // Match the on-device driver: foreign keys are enforced, so an insert that
        // references a not-yet-written word must fail here too.
        driver.execute(null, "PRAGMA foreign_keys = ON;", 0)
        val db = ContentDatabase(driver)
        val content = loadFixture()
        DemoSeedWriter.seed(db, content)
        return db to content
    }

    @Test
    fun `the schema creates and the fixture seeds cleanly`() {
        val (db, content) = seededDatabase()
        assertEquals(content.words.size.toLong(), db.wordsQueries.countAll().executeAsOne())
        assertEquals(content.groups.size.toLong(), db.groupsQueries.countAll().executeAsOne())
        assertTrue(db.sensesQueries.countAll().executeAsOne() >= content.words.size.toLong())
        assertTrue(db.examplesQueries.countAll().executeAsOne() >= content.words.size.toLong())
        assertEquals(
            content.words.size.toLong() * 3,
            db.distractorsQueries.countAll().executeAsOne(),
        )
    }

    @Test
    fun `every word has exactly one primary sense`() {
        val (db, _) = seededDatabase()
        val offenders = db.sensesQueries.selectWordsWithBadPrimaryCount().executeAsList()
        assertTrue(offenders.isEmpty(), "words with a bad primary count: $offenders")
    }

    @Test
    fun `every word has three distractors and none dangle`() {
        val (db, _) = seededDatabase()
        assertTrue(
            db.distractorsQueries.selectWordsWithBadDistractorCount().executeAsList().isEmpty(),
        )
        assertTrue(
            db.distractorsQueries.selectDanglingDistractors().executeAsList().isEmpty(),
        )
    }

    @Test
    fun `learning order is dense, unique and follows the fixture order`() {
        val (db, content) = seededDatabase()
        val rows = db.wordsQueries.selectPlanSlice().executeAsList()
        assertEquals(content.words.size, rows.size)
        assertEquals((1L..content.words.size.toLong()).toList(), rows.map { it.learning_order })
    }

    @Test
    fun `highlight offsets select the target word out of the sentence bytes`() {
        val (db, _) = seededDatabase()
        val words = db.wordsQueries.selectAllOrdered().executeAsList().associateBy { it.word_id }
        db.examplesQueries.selectForWords(words.keys).executeAsList().forEach { example ->
            val bytes = example.sentence.toByteArray(Charsets.UTF_8)
            assertTrue(
                example.hl_end <= bytes.size && example.hl_start < example.hl_end,
                "bad highlight range on example ${example.example_id}",
            )
            val highlighted = String(
                bytes,
                example.hl_start.toInt(),
                (example.hl_end - example.hl_start).toInt(),
                Charsets.UTF_8,
            )
            val word = words.getValue(example.word_id).word
            assertEquals(
                word.lowercase(),
                highlighted.lowercase(),
                "highlight does not cover the target word in example ${example.example_id}",
            )
        }
    }

    @Test
    fun `meta carries the keys the app reads at startup`() {
        val (db, content) = seededDatabase()
        assertEquals(
            content.contentVersion,
            db.metaQueries.selectValue(ContentMetaKeys.CONTENT_VERSION).executeAsOneOrNull(),
        )
        assertNotNull(db.metaQueries.selectValue(ContentMetaKeys.PLAN_ID).executeAsOneOrNull())
        assertNotNull(db.metaQueries.selectValue(ContentMetaKeys.SCHEMA_VER).executeAsOneOrNull())
    }

    @Test
    fun `media filenames are content-addressed and unique per logical asset`() {
        val (db, _) = seededDatabase()
        val words = db.wordsQueries.selectAllOrdered().executeAsList()
        val images = words.map { it.image_file }
        assertEquals(images.size, images.toSet().size, "duplicate image filenames")
        assertTrue(images.all { it.startsWith("${DemoSeedWriter.IMAGE_DIR}/") })

        val audio = buildList {
            addAll(words.map { it.word_audio_file })
            addAll(db.sensesQueries.selectForWords(words.map { it.word_id }).executeAsList()
                .map { it.def_audio_file })
            addAll(db.examplesQueries.selectForWords(words.map { it.word_id }).executeAsList()
                .map { it.ex_audio_file })
        }
        assertEquals(audio.size, audio.toSet().size, "duplicate audio filenames")
        assertTrue(audio.all { it.startsWith("${DemoSeedWriter.AUDIO_DIR}/") })
    }

    @Test
    fun `groups cut into more than one learning unit at the default goal`() {
        // A one-unit demo would never exercise carry-over, which is the interesting part.
        val (db, _) = seededDatabase()
        val groups = db.wordsQueries.selectPlanSlice().executeAsList()
            .groupBy { it.group_id }
        assertTrue(groups.size >= 2, "the demo needs at least two groups to show carry-over")
    }

    @Test
    fun `etymology prose and segments occupy their own columns`() {
        val (db, content) = seededDatabase()
        val rows = db.wordsQueries.selectAllOrdered().executeAsList().associateBy { it.word }
        content.words.forEach { fixture ->
            val row = rows.getValue(fixture.word)
            // Prose goes in verbatim -- no encoded prefix riding along with it.
            assertEquals(fixture.etymology, row.etymology)
            assertEquals(
                fixture.etymologySegments,
                EtymologySegments.parse(row.etymology_segments),
                "segments round-trip failed for ${fixture.word}",
            )
        }
    }

    @Test
    fun `a word with no segmentation stores a null column, not an empty array`() {
        val good = loadFixture()
        val stripped = good.copy(
            words = good.words.mapIndexed { index, w ->
                if (index == 0) w.copy(etymologySegments = emptyList()) else w
            },
        )
        val driver = JdbcSqliteDriver(JdbcSqliteDriver.IN_MEMORY)
        ContentDatabase.Schema.create(driver)
        val db = ContentDatabase(driver)
        DemoSeedWriter.seed(db, stripped)

        val row = db.wordsQueries.selectById(DemoSeedWriter.BASE_WORD_ID).executeAsOne()
        assertNull(row.etymology_segments)
        assertNotNull(row.etymology)
    }

    @Test
    fun `highlight offsets survive multi-byte characters ahead of the target`() {
        val sentence = "Café — a benevolent host waved us in."
        val (start, end) = DemoSeedWriter.byteHighlight(sentence, "benevolent")
        val bytes = sentence.toByteArray(Charsets.UTF_8)
        assertEquals("benevolent", String(bytes, start, end - start, Charsets.UTF_8))
    }

    @Test
    fun `media names are stable and differ per logical key`() {
        val a = DemoSeedWriter.mediaName("img", "benevolent-image", "png")
        val b = DemoSeedWriter.mediaName("img", "benevolent-image", "png")
        val c = DemoSeedWriter.mediaName("img", "malevolent-image", "png")
        assertEquals(a, b)
        assertTrue(a != c)
        assertTrue(a.startsWith("img/") && a.endsWith(".png"))
    }

    @Test
    fun `a malformed fixture is rejected`() {
        val good = loadFixture()
        val broken = good.copy(
            words = good.words.mapIndexed { index, w ->
                if (index == 0) w.copy(distractors = w.distractors.take(2)) else w
            },
        )
        val ids = broken.words
            .mapIndexed { i, w -> w.word.lowercase() to DemoSeedWriter.BASE_WORD_ID + i }
            .toMap()
        val error = runCatching { DemoSeedWriter.validate(broken, ids) }.exceptionOrNull()
        assertNotNull(error)
        assertTrue(error is IllegalArgumentException)
    }
}
