package dev.morpho.domain.model

import java.time.Instant
import java.time.LocalDate

/**
 * User progress model. Mirrors `docs/contracts/user-db.sql`.
 *
 * Progress is keyed on `word_id` only — never on group or learning order, because
 * groups are re-cut per release (README, "组的短暂性" / groups are ephemeral).
 */

enum class LearningStatus {
    LEARNING,
    LEARNED,
    ;

    val dbValue: String get() = name.lowercase()

    companion object {
        fun fromDb(value: String): LearningStatus = when (value) {
            "learning" -> LEARNING
            "learned" -> LEARNED
            else -> error("unknown learning status: $value")
        }
    }
}

/** Learning mode, 1..3 (see README "学习模式"). */
enum class LearnMode(val level: Int) {
    SENTENCE_IMAGE(1),
    WORD_IMAGE_DEF(2),
    WORD_TEXT_DEF(3),
    ;

    fun promoted(): LearnMode = when (this) {
        SENTENCE_IMAGE -> WORD_IMAGE_DEF
        WORD_IMAGE_DEF -> WORD_TEXT_DEF
        WORD_TEXT_DEF -> WORD_TEXT_DEF
    }

    companion object {
        fun fromLevel(level: Int): LearnMode =
            entries.firstOrNull { it.level == level } ?: SENTENCE_IMAGE
    }
}

data class LearningProgress(
    val wordId: Long,
    val currentMode: LearnMode = LearnMode.SENTENCE_IMAGE,
    val roundsPassed: Int = 0,
    val status: LearningStatus = LearningStatus.LEARNING,
)

/** FSRS v5 card state enum, persisted in `fsrs_cards.state`. */
enum class CardState(val code: Int) {
    NEW(0),
    LEARNING(1),
    REVIEW(2),
    RELEARNING(3),
    ;

    companion object {
        fun fromCode(code: Int): CardState =
            entries.firstOrNull { it.code == code } ?: NEW
    }
}

data class FsrsCard(
    val wordId: Long,
    val due: Instant,
    val stability: Double,
    val difficulty: Double,
    val elapsedDays: Int = 0,
    val scheduledDays: Int = 0,
    val reps: Int = 0,
    val lapses: Int = 0,
    val state: CardState = CardState.NEW,
    val lastReview: Instant? = null,
)

/**
 * One row of `daily_stats`.
 *
 * Accuracy is persisted as exact counts rather than a rate (docs/contracts/user-db.sql):
 * merging a second session into the day is then plain addition, with no re-averaging and
 * no drift. [correctRate] stays available as a derived value for the UI.
 */
data class DailyStats(
    val date: LocalDate,
    val newLearned: Int = 0,
    val reviewed: Int = 0,
    val correctCount: Int = 0,
    val answerCount: Int = 0,
) {
    /** 0.0..1.0, or null on a day where nothing was answered. */
    val correctRate: Double?
        get() = if (answerCount == 0) null else correctCount.toDouble() / answerCount
}

// ── Home screen display models ──────────────────────────────────────

enum class GreetingPeriod {
    MORNING,
    AFTERNOON,
    EVENING,
}

enum class ActivityChartStyle {
    BAR,
    HEATMAP,
    ;

    val dbValue: String get() = name.lowercase()

    companion object {
        fun fromDb(value: String): ActivityChartStyle = when (value) {
            "heatmap" -> HEATMAP
            else -> BAR
        }
    }
}

data class DailyActivity(
    val date: LocalDate,
    val wordsStudied: Int,
    val newLearned: Int,
    val reviewed: Int,
)

data class HeatmapCell(
    val date: LocalDate,
    val intensity: Int,
)

data class HeatmapData(
    val cells: List<HeatmapCell>,
    val weeks: Int,
    val maxActivity: Int,
)

/** `meta` keys the app writes into user.db. */
object UserMetaKeys {
    const val CONTENT_VERSION = "content_version"
    const val SCHEMA_VER = "schema_ver"
    const val DAILY_GOAL = "daily_goal"

    // App-owned preferences. Kept in user.db so they ride along with Auto Backup
    // and the manual export/import flow instead of needing a second store.
    const val SOUND_ENABLED = "sound_enabled"
    const val HAPTICS_ENABLED = "haptics_enabled"
    const val SFX_VOLUME = "sfx_volume"
    const val REDUCED_MOTION_OVERRIDE = "reduced_motion_override"
    const val LAST_STUDY_DATE = "last_study_date"
    const val STREAK_DAYS = "streak_days"
    const val ACTIVITY_CHART_STYLE = "activity_chart_style"
}

object ProgressDefaults {
    const val DAILY_GOAL = 50

    /**
     * Cap on one review sitting, as a multiple of the daily new-word goal.
     *
     * FSRS hands back everything that is due, and against a 4,253-word release that is
     * eventually hundreds of cards in a single morning — a queue nobody finishes and a
     * "1 / 863" counter that reads as a punishment. Reviews are naturally a few times the
     * new-word rate, so the goal is the right thing to scale against: raise the goal and
     * the review sitting grows with it. Anything left over stays due and is offered again
     * the moment the session ends, so nothing is dropped, only deferred.
     */
    const val REVIEW_SESSION_MULTIPLIER = 4

    /**
     * user.db schema version.
     *
     * 1 — wave 1: `daily_stats.correct_rate REAL`.
     * 2 — wave 3: `daily_stats.correct_count` + `answer_count`. Pre-release, so the
     *     schema is simply regenerated; there is no v1 data anywhere to migrate.
     *
     * The import flow refuses any file claiming a version above this.
     */
    const val SCHEMA_VER = 2
    const val SFX_VOLUME = 0.8f
}
