package dev.morpho.data.repository

import dev.morpho.domain.model.ActivityChartStyle
import dev.morpho.domain.model.ProgressDefaults
import dev.morpho.domain.model.UserMetaKeys
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/**
 * App settings.
 *
 * Everything lives in `user.db`'s `meta` table rather than DataStore, so preferences
 * ride along with Android Auto Backup and the manual progress export without a second
 * storage mechanism to keep in sync. `daily_goal` is already a contract key; the sound,
 * haptics and motion keys are app-owned additions in the same namespace.
 */
data class MorphoSettings(
    val dailyGoal: Int = ProgressDefaults.DAILY_GOAL,
    val soundEnabled: Boolean = true,
    val hapticsEnabled: Boolean = true,
    val sfxVolume: Float = ProgressDefaults.SFX_VOLUME,
    val reducedMotion: Boolean? = null,
    val activityChartStyle: ActivityChartStyle = ActivityChartStyle.BAR,
)

class SettingsRepository(private val progress: ProgressRepository) {

    private val state = MutableStateFlow(MorphoSettings())
    val settings: StateFlow<MorphoSettings> = state.asStateFlow()

    suspend fun load(): MorphoSettings {
        val loaded = MorphoSettings(
            dailyGoal = progress.metaValue(UserMetaKeys.DAILY_GOAL)?.toIntOrNull()
                ?: ProgressDefaults.DAILY_GOAL,
            soundEnabled = progress.metaValue(UserMetaKeys.SOUND_ENABLED)?.toBooleanStrictOrNull()
                ?: true,
            hapticsEnabled = progress.metaValue(UserMetaKeys.HAPTICS_ENABLED)?.toBooleanStrictOrNull()
                ?: true,
            sfxVolume = progress.metaValue(UserMetaKeys.SFX_VOLUME)?.toFloatOrNull()
                ?: ProgressDefaults.SFX_VOLUME,
            reducedMotion = progress.metaValue(UserMetaKeys.REDUCED_MOTION_OVERRIDE)
                ?.toBooleanStrictOrNull(),
            activityChartStyle = progress.metaValue(UserMetaKeys.ACTIVITY_CHART_STYLE)
                ?.let { ActivityChartStyle.fromDb(it) }
                ?: ActivityChartStyle.BAR,
        )
        state.value = loaded
        return loaded
    }

    suspend fun setDailyGoal(value: Int) {
        val clamped = value.coerceIn(MIN_DAILY_GOAL, MAX_DAILY_GOAL)
        progress.setMeta(UserMetaKeys.DAILY_GOAL, clamped.toString())
        state.value = state.value.copy(dailyGoal = clamped)
    }

    suspend fun setSoundEnabled(value: Boolean) {
        progress.setMeta(UserMetaKeys.SOUND_ENABLED, value.toString())
        state.value = state.value.copy(soundEnabled = value)
    }

    suspend fun setHapticsEnabled(value: Boolean) {
        progress.setMeta(UserMetaKeys.HAPTICS_ENABLED, value.toString())
        state.value = state.value.copy(hapticsEnabled = value)
    }

    suspend fun setSfxVolume(value: Float) {
        val clamped = value.coerceIn(0f, 1f)
        progress.setMeta(UserMetaKeys.SFX_VOLUME, clamped.toString())
        state.value = state.value.copy(sfxVolume = clamped)
    }

    suspend fun setReducedMotion(value: Boolean?) {
        progress.setMeta(UserMetaKeys.REDUCED_MOTION_OVERRIDE, value?.toString() ?: "")
        state.value = state.value.copy(reducedMotion = value)
    }

    suspend fun setActivityChartStyle(value: ActivityChartStyle) {
        progress.setMeta(UserMetaKeys.ACTIVITY_CHART_STYLE, value.dbValue)
        state.value = state.value.copy(activityChartStyle = value)
    }

    companion object {
        const val MIN_DAILY_GOAL = 10
        const val MAX_DAILY_GOAL = 200
        const val DAILY_GOAL_STEP = 10
    }
}
