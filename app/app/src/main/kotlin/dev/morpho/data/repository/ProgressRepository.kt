package dev.morpho.data.repository

import dev.morpho.data.db.user.Daily_stats
import dev.morpho.data.db.user.Fsrs_cards
import dev.morpho.data.db.user.UserDatabase
import dev.morpho.domain.model.CardState
import dev.morpho.domain.model.DailyStats
import dev.morpho.domain.model.FsrsCard
import dev.morpho.domain.model.ProgressDefaults
import dev.morpho.domain.model.UserMetaKeys
import dev.morpho.domain.progress.ProgressTracker
import kotlinx.coroutines.Dispatchers
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

    /**
     * Words the learning-ladder engine left mid-ladder: status `learning` with at least
     * one round passed. Read once, when the stream starts without a saved state.
     */
    suspend fun inFlightWordIds(): List<Long> = withContext(Dispatchers.IO) {
        q.selectInFlight().executeAsList().map { it.word_id }
    }

    // ----------------------------------------------------------------- FSRS

    suspend fun allCards(): List<FsrsCard> = withContext(Dispatchers.IO) {
        cards.selectAll().executeAsList().map { it.toDomain() }
    }

    private fun writeCard(card: FsrsCard) = cards.upsert(
        word_id = card.wordId,
        due = card.due.toString(),
        stability = card.stability,
        difficulty = card.difficulty,
        elapsed_days = card.elapsedDays.toLong(),
        scheduled_days = card.scheduledDays.toLong(),
        reps = card.reps.toLong(),
        lapses = card.lapses.toLong(),
        state = card.state.code.toLong(),
        last_review = card.lastReview?.toString(),
    )

    // ----------------------------------------------------------- daily stats

    suspend fun statsFor(date: LocalDate): DailyStats? = withContext(Dispatchers.IO) {
        stats.selectByDate(date.format(DATE)).executeAsOneOrNull()?.toDomain()
    }

    suspend fun recentStats(limit: Int = 400): List<DailyStats> = withContext(Dispatchers.IO) {
        stats.selectRecent(limit.toLong()).executeAsList().map { it.toDomain() }
    }

    suspend fun upsertStats(row: DailyStats) = withContext(Dispatchers.IO) { writeStats(row) }

    private fun writeStats(row: DailyStats) = stats.upsert(
        date = row.date.format(DATE),
        new_learned = row.newLearned.toLong(),
        reviewed = row.reviewed.toLong(),
        correct_count = row.correctCount.toLong(),
        answer_count = row.answerCount.toLong(),
    )

    // ------------------------------------------------------------ stream step

    /**
     * Writes one finished stream step in a single transaction: the cards it [changed], the
     * counts it adds to [delta]'s day, and the [metaRows] that hold the stream state.
     * A delta of all zeros leaves `daily_stats` alone.
     */
    suspend fun recordStep(
        changed: Collection<FsrsCard>,
        delta: DailyStats,
        metaRows: Map<String, String>,
    ) = withContext(Dispatchers.IO) {
        db.transaction {
            changed.forEach(::writeCard)
            val counts = delta.newLearned + delta.reviewed + delta.correctCount + delta.answerCount
            if (counts > 0) {
                val existing = stats.selectByDate(delta.date.format(DATE)).executeAsOneOrNull()?.toDomain()
                writeStats(
                    ProgressTracker.mergeSession(
                        existing = existing,
                        date = delta.date,
                        newLearned = delta.newLearned,
                        reviewed = delta.reviewed,
                        correctAnswers = delta.correctCount,
                        totalAnswers = delta.answerCount,
                    ),
                )
            }
            metaRows.forEach { (key, value) -> meta.upsert(key, value) }
        }
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
                // The stamp always tracks the schema the app is compiled against. A
                // lower value can only come from a pre-release build, whose tables are
                // regenerated rather than migrated; a higher one means the file was
                // written by a newer app, which the import flow already refuses.
                val stamped = meta.selectValue(UserMetaKeys.SCHEMA_VER)
                    .executeAsOneOrNull()?.toIntOrNull()
                if (stamped != ProgressDefaults.SCHEMA_VER) {
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
    correctCount = correct_count.toInt(),
    answerCount = answer_count.toInt(),
)
