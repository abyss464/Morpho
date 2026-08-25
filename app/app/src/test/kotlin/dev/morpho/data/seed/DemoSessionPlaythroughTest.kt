package dev.morpho.data.seed

import app.cash.sqldelight.driver.jdbc.sqlite.JdbcSqliteDriver
import dev.morpho.data.db.content.ContentDatabase
import dev.morpho.data.repository.ContentRepository
import dev.morpho.domain.learning.LearningEngine
import dev.morpho.domain.learning.OptionAssembler
import dev.morpho.domain.learning.SessionConfig
import dev.morpho.domain.model.LearnMode
import dev.morpho.domain.model.LearningProgress
import java.io.File
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.json.Json
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNotNull
import kotlin.test.assertTrue

/**
 * Plays an entire demo session end to end against the seeded database: the engine
 * chooses the questions, the repository loads the content, and the option assembler
 * builds every four-way choice.
 *
 * This is the wave-1 proof that the app is actually demoable — if a distractor were
 * missing content, or the mode ladder never reached mode 3, this test would fail long
 * before anyone installed the APK.
 */
class DemoSessionPlaythroughTest {

    private fun repository(): ContentRepository {
        val driver = JdbcSqliteDriver(JdbcSqliteDriver.IN_MEMORY)
        ContentDatabase.Schema.create(driver)
        // Match the on-device driver: foreign keys are enforced, so an insert that
        // references a not-yet-written word must fail here too.
        driver.execute(null, "PRAGMA foreign_keys = ON;", 0)
        val db = ContentDatabase(driver)
        val content: DemoContent = Json { ignoreUnknownKeys = true }
            .decodeFromString(File("src/main/assets/${DemoContentSeeder.ASSET}").readText())
        DemoSeedWriter.seed(db, content)
        return ContentRepository(db)
    }

    @Test
    fun `a perfect session clears every demo word through all three modes`() = runBlocking {
        val repo = repository()
        val config = SessionConfig(sessionSeed = 20260826L)
        val plan = LearningEngine.buildSession(
            plan = repo.planWords(),
            progress = emptyMap(),
            dueReviewIds = emptyList(),
            newWordQuota = 50,
            config = config,
        )
        assertTrue(plan.units.size >= 2, "demo should produce more than one learning unit")

        var state = LearningEngine.startSession(plan, emptyMap(), config)
        val modesSeen = mutableMapOf<Long, MutableSet<LearnMode>>()
        var questions = 0

        while (!state.finished && state.currentQuestion != null) {
            val q = state.currentQuestion!!
            modesSeen.getOrPut(q.wordId) { mutableSetOf() } += q.mode

            // Everything the screen would need must resolve.
            val content = repo.questionBundles(q.wordId)
            assertNotNull(content, "no content for word ${q.wordId}")
            assertEquals(4, content.options.size)

            val options = OptionAssembler.assemble(
                answerWordId = q.wordId,
                distractorIds = content.answer.distractorIds,
                seed = q.optionSeed(config.sessionSeed),
            )
            assertEquals(4, options.toSet().size, "duplicate option ids")
            assertTrue(q.wordId in options)

            when (q.mode) {
                LearnMode.SENTENCE_IMAGE ->
                    assertNotNull(content.answer.mode1Example, "mode 1 needs a sentence")
                LearnMode.WORD_IMAGE_DEF, LearnMode.WORD_TEXT_DEF ->
                    assertTrue(content.options.all { it.primarySense.definition.isNotBlank() })
            }

            state = LearningEngine.submitAnswer(state, correct = true).state
            questions++
            assertTrue(questions < 1000, "session did not terminate")
        }

        val totalWords = repo.planWords().size
        assertEquals(totalWords, state.learnedThisSession.size)
        assertEquals(totalWords * 3, questions)
        assertTrue(
            modesSeen.values.all { it.size == 3 },
            "some words never reached all three modes",
        )
    }

    @Test
    fun `every distractor of every word resolves to shipped content`() = runBlocking {
        val repo = repository()
        repo.planWords().forEach { planWord ->
            val content = repo.questionBundles(planWord.wordId)
            assertNotNull(content, "distractor closure broken at word ${planWord.wordId}")
            content.options.forEach { option ->
                assertTrue(option.word.imageFile.isNotBlank())
                assertTrue(option.word.wordAudioFile.isNotBlank())
                assertTrue(option.primarySense.definition.isNotBlank())
            }
        }
        assertTrue(repo.assertIntegrity().isEmpty())
    }

    @Test
    fun `a session resumed mid-ladder keeps its banked modes`() = runBlocking {
        val repo = repository()
        val plan = repo.planWords()
        val partial = plan.take(3).associate { pw ->
            pw.wordId to LearningProgress(
                wordId = pw.wordId,
                currentMode = LearnMode.WORD_TEXT_DEF,
                roundsPassed = 2,
            )
        }
        val config = SessionConfig(sessionSeed = 1L)
        val session = LearningEngine.buildSession(
            plan = plan,
            progress = partial,
            dueReviewIds = emptyList(),
            newWordQuota = 50,
            config = config,
        )
        val state = LearningEngine.startSession(session, partial, config)
        partial.keys.forEach { id ->
            val card = state.current!!.cards[id]
            assertNotNull(card)
            assertEquals(LearnMode.WORD_TEXT_DEF, card.mode)
            assertEquals(2, card.roundsPassed)
        }
    }

    @Test
    fun `media manifest covers every referenced file exactly once`() = runBlocking {
        val repo = repository()
        val manifest = repo.allMediaFiles()
        assertEquals(repo.planWords().size, manifest.images.size)
        assertTrue(manifest.audio.size >= repo.planWords().size * 2)
        assertTrue(manifest.images.all { it.startsWith("img/") })
        assertTrue(manifest.audio.all { it.startsWith("audio/") })
    }
}
