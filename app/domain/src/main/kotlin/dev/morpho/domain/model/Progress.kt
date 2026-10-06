package dev.morpho.domain.model

import java.time.Instant
import java.time.LocalDate

/**
 * User progress model. Mirrors `docs/contracts/user-db.sql`.
 *
 * Progress is keyed on `word_id` only — never on group or learning order, because
 * groups are re-cut per release (README, "组的短暂性" / groups are ephemeral).
 */

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

/**
 * Which colour scheme the app paints in.
 *
 * [SYSTEM] is the default and follows the platform's day/night setting; [LIGHT] and
 * [DARK] pin it. Stored in `user.db`'s `meta` like every other preference, so the
 * choice survives Auto Backup and the manual export.
 */
enum class ThemeMode {
    SYSTEM,
    LIGHT,
    DARK,
    ;

    val dbValue: String get() = name.lowercase()

    /** Resolve to an actual scheme given what the platform currently asks for. */
    fun isDark(systemInDark: Boolean): Boolean = when (this) {
        SYSTEM -> systemInDark
        LIGHT -> false
        DARK -> true
    }

    companion object {
        /** Anything unrecognised — including a cleared value — falls back to [SYSTEM]. */
        fun fromDb(value: String): ThemeMode = when (value) {
            "light" -> LIGHT
            "dark" -> DARK
            else -> SYSTEM
        }
    }
}

/** One day of the activity chart: how many graded steps were answered. */
data class DailyActivity(
    val date: LocalDate,
    val answers: Int,
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
    const val THEME_MODE = "theme_mode"

    /** The stream's state as JSON: word stages, today's counters and the step on screen. */
    const val STREAM_STATE = "stream_state"

    /** The learner's own explanations, per word, as JSON. */
    const val STREAM_NOTES = "stream_notes"

    /** The web app's address progress syncs with (docs/contracts/sync.md); absent until the first sync. */
    const val SYNC_ADDRESS = "sync_address"
}

object ProgressDefaults {
    /** New words a day (docs/contracts/stream.md §5, N). */
    const val DAILY_GOAL = 20

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
