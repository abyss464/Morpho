package dev.morpho.ui.settings

import android.net.Uri
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import dev.morpho.data.backup.BackupVerdict
import dev.morpho.data.backup.StagedBackup
import dev.morpho.data.haptics.HapticPattern
import dev.morpho.data.repository.MorphoSettings
import dev.morpho.data.repository.SettingsRepository
import dev.morpho.data.sound.SfxEvent
import dev.morpho.di.AppContainer
import dev.morpho.domain.model.ActivityChartStyle
import dev.morpho.domain.model.ThemeMode
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

/**
 * Settings changes take effect immediately: the sound and haptics managers are updated
 * in the same call that persists the value, and each toggle plays its own effect so the
 * user hears/feels exactly what they just turned on.
 */
class SettingsViewModel(private val container: AppContainer) : ViewModel() {

    private val repo: SettingsRepository = container.settingsRepository

    val settings: StateFlow<MorphoSettings> = repo.settings

    private val _backup = MutableStateFlow(BackupUiState())
    val backup: StateFlow<BackupUiState> = _backup.asStateFlow()

    fun setDailyGoal(value: Int) {
        val snapped = (value / SettingsRepository.DAILY_GOAL_STEP) *
            SettingsRepository.DAILY_GOAL_STEP
        viewModelScope.launch { repo.setDailyGoal(snapped) }
    }

    fun setSoundEnabled(value: Boolean) {
        container.soundManager.enabled = value
        viewModelScope.launch { repo.setSoundEnabled(value) }
        if (value) container.playSfx(SfxEvent.CORRECT)
    }

    fun setSfxVolume(value: Float) {
        container.soundManager.volume = value
        viewModelScope.launch { repo.setSfxVolume(value) }
        container.playSfx(SfxEvent.TAP)
    }

    fun setHapticsEnabled(value: Boolean) {
        container.hapticsManager.enabled = value
        viewModelScope.launch { repo.setHapticsEnabled(value) }
        if (value) container.hapticsManager.perform(HapticPattern.PROMOTE)
    }

    fun setReducedMotion(value: Boolean?) {
        viewModelScope.launch { repo.setReducedMotion(value) }
    }

    fun setActivityChartStyle(value: ActivityChartStyle) {
        viewModelScope.launch { repo.setActivityChartStyle(value) }
    }

    /**
     * Repaints the whole app the moment it lands: `MainActivity` collects the same
     * settings flow, so nothing here has to reach for the theme.
     */
    fun setThemeMode(value: ThemeMode) {
        viewModelScope.launch { repo.setThemeMode(value) }
        container.playSfx(SfxEvent.TAP)
    }

    // ---------------------------------------------------------------- backup

    fun suggestedExportName(): String = container.progressBackup.suggestedFileName()

    /** The user picked a destination from `ACTION_CREATE_DOCUMENT`. */
    fun onExportTarget(target: Uri?) {
        if (target == null) return
        _backup.value = BackupUiState(busy = true)
        viewModelScope.launch {
            val result = container.progressBackup.export(target)
            _backup.value = BackupUiState(
                message = result.fold(
                    onSuccess = { BackupMessage.Exported(it) },
                    onFailure = { BackupMessage.ExportFailed },
                ),
            )
            if (result.isSuccess) container.playSfx(SfxEvent.REVIEW_DONE)
        }
    }

    /** The user picked a file from `ACTION_OPEN_DOCUMENT`. Stages and validates only. */
    fun onImportSource(source: Uri?) {
        if (source == null) return
        _backup.value = BackupUiState(busy = true)
        viewModelScope.launch {
            val staged = container.progressBackup.stage(source)
            _backup.value = when (val verdict = staged.verdict) {
                is BackupVerdict.Ok -> BackupUiState(pending = staged)
                else -> BackupUiState(message = BackupMessage.Rejected(verdict))
            }
        }
    }

    fun cancelImport() {
        val staged = _backup.value.pending ?: return
        _backup.value = BackupUiState()
        viewModelScope.launch { container.progressBackup.discard(staged) }
    }

    /**
     * Swaps the file in and restarts. On the happy path nothing after this runs, so
     * there is deliberately no success state to render — only the refusal.
     */
    fun confirmImport() {
        val staged = _backup.value.pending ?: return
        _backup.value = BackupUiState(busy = true)
        viewModelScope.launch {
            if (!container.progressBackup.applyAndRestart(staged)) {
                _backup.value = BackupUiState(message = BackupMessage.ImportFailed)
            }
        }
    }

    fun dismissMessage() {
        _backup.value = _backup.value.copy(message = null)
    }

    companion object {
        fun factory(container: AppContainer) = viewModelFactory {
            initializer { SettingsViewModel(container) }
        }
    }
}

data class BackupUiState(
    val busy: Boolean = false,
    /** Non-null while the confirm dialog is up. */
    val pending: StagedBackup? = null,
    val message: BackupMessage? = null,
)

sealed interface BackupMessage {
    data class Exported(val bytes: Long) : BackupMessage
    data object ExportFailed : BackupMessage
    data class Rejected(val verdict: BackupVerdict) : BackupMessage
    data object ImportFailed : BackupMessage
}
