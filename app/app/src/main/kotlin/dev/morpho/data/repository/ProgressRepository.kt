package dev.morpho.data.repository

import app.cash.sqldelight.coroutines.asFlow
import app.cash.sqldelight.coroutines.mapToList
import dev.morpho.data.db.user.Daily_stats
import dev.morpho.data.db.user.Fsrs_cards
import dev.morpho.data.db.user.Learning_progress
import dev.morpho.data.db.user.UserDatabase
import dev.morpho.domain.model.CardState
import dev.morpho.domain.model.DailyStats
import dev.morpho.domain.model.FsrsCard
import dev.morpho.domain.model.LearnMode
import dev.morpho.domain.model.LearningProgress
import dev.morpho.domain.model.LearningStatus
import dev.morpho.domain.model.ProgressDefaults
import dev.morpho.domain.model.UserMetaKeys
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.withContext
import java.time.Instant
import java.time.LocalDate
import java.time.format.DateTimeFormatter

/**
 * Read/write access to user.db.
 *
 * Progress rows are **never deleted** (docs/contracts/user-db.sql): a word dropped by a
 * later release simply stops matching the join against release.db, and comes back with
 * its FSRS state intact if a future release ships it again.
 */
class ProgressRepository(private val db: UserDatabase) {

    private val q = db.learningProgressQueries
    private val cards = db.fsrsCardsQueries
    private val stats = db.dailyStatsQueries
    private val meta = db.userMetaQueries

    // ------------------------------------------------------------- learning

    fun observeProgress(): Flow<List<LearningProgress>> =
        q.selectAll().asFlow().mapToList(Dispatchers.IO)
            .map { rows -> rows.map { it.toDomain() } }

    suspend fun allProgress(): List<LearningProgress> = withContext(Dispatchers.IO) {
        q.selectAll().executeAsList().map { it.toDomain() }
    }

    suspend fun progressFor(wordIds: Collection<Long>): Map<Long, LearningProgress> {
        if (wordIds.isEmpty()) return emptyMap()
        return withContext(Dispatchers.IO) {
            q.selectByIds(wordIds).executeAsList().associate { it.word_id to it.toDomain() }
        }
    }

    suspend fun progressMap(): Map<Long, LearningProgress> =
        allProgress().associateBy { it.wordId }

    suspend fun learnedCount(): Int = withContext(Dispatchers.IO) {
        q.countLearned().executeAsOne().toInt()
    }

    suspend fun upsertProgress(rows: Collection<LearningProgress>) {
        if (rows.isEmpty()) return
        withContext(Dispatchers.IO) {
            db.transaction {
                rows.forEach {
                    q.upsert(
                        word_id = it.wordId,
                        current_mode = it.currentMode.level.toLong(),
                        rounds_passed = it.roundsPassed.toLong(),
                        status = it.status.dbValue,
                    )
                }
            }
        }
    }

    // ----------------------------------------------------------------- FSRS

    suspend fun allCards(): List<FsrsCard> = withContext(Dispatchers.IO) {
        cards.selectAll().executeAsList().map { it.toDomain() }
    }

    suspend fun card(wordId: Long): FsrsCard? = withContext(Dispatchers.IO) {
        cards.selectById(wordId).executeAsOneOrNull()?.toDomain()
    }

    suspend fun dueCards(now: Instant): List<FsrsCard> = withContext(Dispatchers.IO) {
        cards.selectDue(now.toString()).executeAsList().map { it.toDomain() }
    }

    suspend fun dueCount(now: Instant): Int = withContext(Dispatchers.IO) {
        cards.countDue(now.toString()).executeAsOne().toInt()
    }

    suspend fun upsertCards(rows: Collection<FsrsCard>) {
        if (rows.isEmpty()) return
        withContext(Dispatchers.IO) {
            db.transaction {
                rows.forEach {
                    cards.upsert(
                        word_id = it.wordId,
                        due = it.due.toString(),
                        stability = it.stability,
                        difficulty = it.difficulty,
                        elapsed_days = it.elapsedDays.toLong(),
                        scheduled_days = it.scheduledDays.toLong(),
                        reps = it.reps.toLong(),
                        lapses = it.lapses.toLong(),
                        state = it.state.code.toLong(),
                        last_review = it.lastReview?.toString(),
                    )
                }
            }
        }
    }

    // ----------------------------------------------------------- daily stats

    suspend fun statsFor(date: LocalDate): DailyStats? = withContext(Dispatchers.IO) {
        stats.selectByDate(date.format(DATE)).executeAsOneOrNull()?.toDomain()
    }

    suspend fun recentStats(limit: Int = 400): List<DailyStats> = withContext(Dispatchers.IO) {
        stats.selectRecent(limit.toLong()).executeAsList().map { it.toDomain() }
    }

    suspend fun upsertStats(row: DailyStats) = withContext(Dispatchers.IO) {
        stats.upsert(
            date = row.date.format(DATE),
            new_learned = row.newLearned.toLong(),
            reviewed = row.reviewed.toLong(),
            correct_rate = row.correctRate,
        )
    }

    // ------------------------------------------------------------------ meta

    suspend fun metaValue(key: String): String? = withContext(Dispatchers.IO) {
        meta.selectValue(key).executeAsOneOrNull()
    }

    suspend fun setMeta(key: String, value: String) = withContext(Dispatchers.IO) {
        meta.upsert(key, value)
    }

    suspend fun ensureInitialised(contentVersion: String?) {
        withContext(Dispatchers.IO) {
            db.transaction {
                if (meta.selectValue(UserMetaKeys.SCHEMA_VER).executeAsOneOrNull() == null) {
                    meta.upsert(UserMetaKeys.SCHEMA_VER, ProgressDefaults.SCHEMA_VER.toString())
                }
                if (meta.selectValue(UserMetaKeys.DAILY_GOAL).executeAsOneOrNull() == null) {
                    meta.upsert(UserMetaKeys.DAILY_GOAL, ProgressDefaults.DAILY_GOAL.toString())
                }
                if (contentVersion != null) {
                    // Content-version reconciliation is purely a meta write: progress
                    // rows are keyed on word_id and survive any release change untouched
                    // (README Part 6, "内容版本对账").
                    meta.upsert(UserMetaKeys.CONTENT_VERSION, contentVersion)
                }
            }
        }
    }

    /** Wipes every table. Used by the import flow before restoring a snapshot. */
    suspend fun clearAll() = withContext(Dispatchers.IO) {
        db.transaction {
            q.deleteAll()
            cards.deleteAll()
            stats.deleteAll()
            meta.deleteAll()
        }
    }

    companion object {
        private val DATE: DateTimeFormatter = DateTimeFormatter.ISO_LOCAL_DATE
    }
}

// ------------------------------------------------------------------ mapping

private fun Learning_progress.toDomain() = LearningProgress(
    wordId = word_id,
    currentMode = LearnMode.fromLevel(current_mode.toInt()),
    roundsPassed = rounds_passed.toInt(),
    status = LearningStatus.fromDb(status),
)

private fun Fsrs_cards.toDomain() = FsrsCard(
    wordId = word_id,
    due = Instant.parse(due),
    stability = stability,
    difficulty = difficulty,
    elapsedDays = elapsed_days.toInt(),
    scheduledDays = scheduled_days.toInt(),
    reps = reps.toInt(),
    lapses = lapses.toInt(),
    state = CardState.fromCode(state.toInt()),
    lastReview = last_review?.let(Instant::parse),
)

private fun Daily_stats.toDomain() = DailyStats(
    date = LocalDate.parse(date),
    newLearned = new_learned.toInt(),
    reviewed = reviewed.toInt(),
    correctRate = correct_rate,
)
