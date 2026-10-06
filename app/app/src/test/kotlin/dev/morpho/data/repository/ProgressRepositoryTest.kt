package dev.morpho.data.repository

import app.cash.sqldelight.driver.jdbc.sqlite.JdbcSqliteDriver
import dev.morpho.data.db.user.UserDatabase
import dev.morpho.domain.progress.ProgressTracker
import java.time.LocalDate
import kotlin.test.AfterTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlinx.coroutines.runBlocking

/**
 * Real SQLite (JDBC, no device, no Robolectric — see ReleaseDatabaseTest) round trip
 * for `daily_stats`: counts folded with [ProgressTracker.mergeSession] and written with
 * `upsertStats` are read back by `statsFor`, against a freshly created `user.db` schema.
 */
class ProgressRepositoryTest {

    private val driver = JdbcSqliteDriver(JdbcSqliteDriver.IN_MEMORY).also {
        UserDatabase.Schema.create(it)
    }
    private val repo = ProgressRepository(UserDatabase(driver))

    @AfterTest
    fun tearDown() = driver.close()

    @Test
    fun `a finished learning session is read back by the next home refresh`() = runBlocking {
        val today = LocalDate.now()

        // Before any session: a fresh day reads as zero against the default goal —
        // the exact state the bug report describes as permanently stuck.
        assertEquals(0, repo.statsFor(today)?.newLearned ?: 0)

        // Fold a batch of counts into today's row and persist it.
        val merged = ProgressTracker.mergeSession(
            existing = repo.statsFor(today),
            date = today,
            newLearned = 20,
            reviewed = 0,
            correctAnswers = 23,
            totalAnswers = 25,
        )
        repo.upsertStats(merged)

        // The next read sees it.
        assertEquals(20, repo.statsFor(today)?.newLearned)
    }

    @Test
    fun `a same-day review session merges onto what learning already wrote`() = runBlocking {
        val today = LocalDate.now()

        repo.upsertStats(
            ProgressTracker.mergeSession(
                existing = repo.statsFor(today),
                date = today,
                newLearned = 20,
                reviewed = 0,
                correctAnswers = 18,
                totalAnswers = 20,
            ),
        )

        // A later batch the same day adds, never overwrites.
        repo.upsertStats(
            ProgressTracker.mergeSession(
                existing = repo.statsFor(today),
                date = today,
                newLearned = 0,
                reviewed = 12,
                correctAnswers = 10,
                totalAnswers = 12,
            ),
        )

        val state = repo.statsFor(today)
        assertEquals(20, state?.newLearned)
        assertEquals(12, state?.reviewed)
    }

    @Test
    fun `yesterday's row is untouched by a session recorded today`() = runBlocking {
        val today = LocalDate.now()
        val yesterday = today.minusDays(1)

        repo.upsertStats(
            ProgressTracker.mergeSession(
                existing = repo.statsFor(yesterday),
                date = yesterday,
                newLearned = 50,
                reviewed = 0,
                correctAnswers = 45,
                totalAnswers = 50,
            ),
        )
        repo.upsertStats(
            ProgressTracker.mergeSession(
                existing = repo.statsFor(today),
                date = today,
                newLearned = 5,
                reviewed = 0,
                correctAnswers = 5,
                totalAnswers = 5,
            ),
        )

        assertEquals(50, repo.statsFor(yesterday)?.newLearned)
        assertEquals(5, repo.statsFor(today)?.newLearned)
    }
}
