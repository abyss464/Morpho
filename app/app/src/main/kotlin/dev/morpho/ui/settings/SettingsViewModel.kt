package dev.morpho.ui.settings

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import dev.morpho.data.haptics.HapticPattern
import dev.morpho.data.repository.MorphoSettings
import dev.morpho.data.repository.SettingsRepository
import dev.morpho.data.sound.SfxEvent
import dev.morpho.di.AppContainer
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch

/**
 * Settings changes take effect immediately: the sound and haptics managers are updated
 * in the same call that persists the value, and each toggle plays its own effect so the
 * user hears/feels exactly what they just turned on.
 */
class SettingsViewModel(private val container: AppContainer) : ViewModel() {

    private val repo: SettingsRepository = container.settingsRepository

    val settings: StateFlow<MorphoSettings> = repo.settings

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

    companion object {
        fun factory(container: AppContainer) = viewModelFactory {
            initializer { SettingsViewModel(container) }
        }
    }
}
