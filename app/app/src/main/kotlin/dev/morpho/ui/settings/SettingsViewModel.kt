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
import dev.morpho.data.sync.ProgressSync
import dev.morpho.data.sync.SyncOutcome
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

    /** The address field: the saved sync address, else the default. */
    private val _syncAddress = MutableStateFlow(repo.settings.value.syncAddress ?: ProgressSync.DEFAULT_ADDRESS)
    val syncAddress: StateFlow<String> = _syncAddress.asStateFlow()

    private val _syncing = MutableStateFlow(false)
    val syncing: StateFlow<Boolean> = _syncing.asStateFlow()

    /** The last sync's outcome, this one or Today's silent one. */
    val syncOutcome: StateFlow<SyncOutcome?> = container.progressSync.last

    fun setSyncAddress(value: String) {
        _syncAddress.value = value
    }

    /**
     * "Sync now": saves the address and syncs with it. The field then shows the address as
     * saved (scheme added, trailing slash dropped), unless it was edited meanwhile; text that
     * is not an address stays as typed, under its error.
     */
    fun syncNow() {
        if (_syncing.value) return
        container.tap()
        _syncing.value = true
        val typed = _syncAddress.value
        viewModelScope.launch {
            container.progressSync.syncWith(typed)
            ProgressSync.normalizeAddress(typed)?.let { if (_syncAddress.value == typed) _syncAddress.value = it }
            _syncing.value = false
        }
    }

    /** A tick for each step the slider passes, not for every pixel it moves. */
    fun setDailyGoal(value: Int) {
        val snapped = (value / SettingsRepository.DAILY_GOAL_STEP) *
            SettingsRepository.DAILY_GOAL_STEP
        if (snapped != settings.value.dailyGoal) container.tap()
        viewModelScope.launch { repo.setDailyGoal(snapped) }
    }

    fun setSoundEnabled(value: Boolean) {
        container.soundManager.enabled = value
        viewModelScope.launch { repo.setSoundEnabled(value) }
        container.hapticsManager.perform(HapticPattern.TAP)
        if (value) container.playSfx(SfxEvent.CORRECT)
    }

    /** Plays the tap at the new volume each time the slider crosses a tenth. */
    fun setSfxVolume(value: Float) {
        val crossed = (value * VOLUME_STEPS).toInt() != (settings.value.sfxVolume * VOLUME_STEPS).toInt()
        container.soundManager.volume = value
        viewModelScope.launch { repo.setSfxVolume(value) }
        if (crossed) container.tap()
    }

    fun setHapticsEnabled(value: Boolean) {
        container.hapticsManager.enabled = value
        viewModelScope.launch { repo.setHapticsEnabled(value) }
        container.playSfx(SfxEvent.TAP)
        if (value) container.hapticsManager.perform(HapticPattern.PROMOTE)
    }

    fun setReducedMotion(value: Boolean?) {
        container.tap()
        viewModelScope.launch { repo.setReducedMotion(value) }
    }

    fun setActivityChartStyle(value: ActivityChartStyle) {
        container.tap()
        viewModelScope.launch { repo.setActivityChartStyle(value) }
    }

    /** Feedback for a press that opens something (the file pickers, a dialog's buttons). */
    fun onTap() = container.tap()

    /**
     * Repaints the whole app the moment it lands: `MainActivity` collects the same
     * settings flow, so nothing here has to reach for the theme.
     */
    fun setThemeMode(value: ThemeMode) {
        viewModelScope.launch { repo.setThemeMode(value) }
        container.tap()
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
        container.tap()
        _backup.value = BackupUiState()
        viewModelScope.launch { container.progressBackup.discard(staged) }
    }

    /**
     * Swaps the file in and restarts. On the happy path nothing after this runs, so
     * there is deliberately no success state to render — only the refusal.
     */
    fun confirmImport() {
        val staged = _backup.value.pending ?: return
        container.tap()
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
        private const val VOLUME_STEPS = 10

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
