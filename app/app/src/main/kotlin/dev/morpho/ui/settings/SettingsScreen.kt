package dev.morpho.ui.settings

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.ArrowBack
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Slider
import androidx.compose.material3.SliderDefaults
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import dev.morpho.BuildConfig
import dev.morpho.R
import dev.morpho.data.repository.MorphoSettings
import dev.morpho.data.repository.SettingsRepository
import dev.morpho.data.sound.SfxEvent
import dev.morpho.di.AppContainer
import dev.morpho.di.StartupReport
import dev.morpho.ui.designsystem.component.PreviewBox
import dev.morpho.ui.designsystem.component.ThemePreviews
import dev.morpho.ui.designsystem.theme.MorphoTheme

/**
 * Settings: daily goal, the sound and haptics toggles the design contract requires,
 * and the progress export/import entry points (stubbed in wave 1 — the SAF plumbing
 * lands with the backup work).
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SettingsScreen(
    container: AppContainer,
    startup: StartupReport,
    onBack: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val viewModel: SettingsViewModel = viewModel(factory = SettingsViewModel.factory(container))
    val settings by viewModel.settings.collectAsStateWithLifecycle()
    var stubNotice by remember { mutableStateOf(false) }

    Scaffold(
        modifier = modifier.fillMaxSize(),
        topBar = {
            TopAppBar(
                title = { Text(stringResource(R.string.settings_title)) },
                navigationIcon = {
                    IconButton(onClick = {
                        container.playSfx(SfxEvent.TAP)
                        onBack()
                    }) {
                        Icon(
                            Icons.AutoMirrored.Rounded.ArrowBack,
                            contentDescription = stringResource(R.string.action_back),
                        )
                    }
                },
            )
        },
    ) { padding ->
        SettingsContent(
            settings = settings,
            wordCount = startup.wordCount,
            contentVersion = startup.contentVersion,
            stubNoticeVisible = stubNotice,
            modifier = Modifier
                .fillMaxSize()
                .padding(padding),
            onDailyGoalChange = viewModel::setDailyGoal,
            onSoundChange = viewModel::setSoundEnabled,
            onVolumeChange = viewModel::setSfxVolume,
            onHapticsChange = viewModel::setHapticsEnabled,
            onReducedMotionChange = viewModel::setReducedMotion,
            onStubTapped = { stubNotice = true },
        )
    }
}

@Composable
private fun SettingsContent(
    settings: MorphoSettings,
    wordCount: Int,
    contentVersion: String?,
    stubNoticeVisible: Boolean,
    onDailyGoalChange: (Int) -> Unit,
    onSoundChange: (Boolean) -> Unit,
    onVolumeChange: (Float) -> Unit,
    onHapticsChange: (Boolean) -> Unit,
    onReducedMotionChange: (Boolean?) -> Unit,
    onStubTapped: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val spacing = MorphoTheme.spacing
    Column(
        modifier = modifier
            .verticalScroll(rememberScrollState())
            .padding(horizontal = spacing.screenGutter)
            .padding(bottom = spacing.xxl),
        verticalArrangement = Arrangement.spacedBy(spacing.md),
    ) {
        SectionHeader(stringResource(R.string.settings_section_learning))

        Column(verticalArrangement = Arrangement.spacedBy(spacing.xxs)) {
            SettingRow(
                title = stringResource(R.string.settings_daily_goal),
                summary = stringResource(R.string.settings_daily_goal_summary, settings.dailyGoal),
            )
            Slider(
                value = settings.dailyGoal.toFloat(),
                onValueChange = { onDailyGoalChange(it.toInt()) },
                valueRange = SettingsRepository.MIN_DAILY_GOAL.toFloat()..
                    SettingsRepository.MAX_DAILY_GOAL.toFloat(),
                steps = (SettingsRepository.MAX_DAILY_GOAL - SettingsRepository.MIN_DAILY_GOAL) /
                    SettingsRepository.DAILY_GOAL_STEP - 1,
                colors = morphoSliderColors(),
                modifier = Modifier.fillMaxWidth(),
            )
        }

        HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)
        SectionHeader(stringResource(R.string.settings_section_feedback))

        ToggleRow(
            title = stringResource(R.string.settings_sound),
            summary = stringResource(R.string.settings_sound_summary),
            checked = settings.soundEnabled,
            onCheckedChange = onSoundChange,
        )
        if (settings.soundEnabled) {
            Column(verticalArrangement = Arrangement.spacedBy(spacing.xxs)) {
                SettingRow(title = stringResource(R.string.settings_sfx_volume))
                Slider(
                    value = settings.sfxVolume,
                    onValueChange = onVolumeChange,
                    colors = morphoSliderColors(),
                    modifier = Modifier.fillMaxWidth(),
                )
            }
        }
        ToggleRow(
            title = stringResource(R.string.settings_haptics),
            summary = stringResource(R.string.settings_haptics_summary),
            checked = settings.hapticsEnabled,
            onCheckedChange = onHapticsChange,
        )
        ToggleRow(
            title = stringResource(R.string.settings_reduced_motion),
            summary = stringResource(R.string.settings_reduced_motion_summary),
            checked = settings.reducedMotion == true,
            onCheckedChange = { onReducedMotionChange(if (it) true else null) },
        )

        HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)
        SectionHeader(stringResource(R.string.settings_section_data))

        SettingRow(
            title = stringResource(R.string.settings_export),
            summary = stringResource(R.string.settings_export_summary),
            onClick = onStubTapped,
        )
        SettingRow(
            title = stringResource(R.string.settings_import),
            summary = stringResource(R.string.settings_import_summary),
            onClick = onStubTapped,
        )
        if (stubNoticeVisible) {
            Text(
                text = stringResource(R.string.settings_stub_notice),
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.secondary,
            )
        }

        HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)
        SectionHeader(stringResource(R.string.settings_section_about))

        SettingRow(
            title = stringResource(R.string.settings_word_count),
            summary = wordCount.toString(),
        )
        SettingRow(
            title = stringResource(R.string.settings_content_version),
            summary = contentVersion ?: "—",
        )
        SettingRow(
            title = stringResource(R.string.settings_app_version),
            summary = "${BuildConfig.VERSION_NAME} (${BuildConfig.VERSION_CODE})",
        )
    }
}

/**
 * Neutral slider track. The M3 default resolves to the secondary container, which is
 * teal in this brand and reads as a second value rather than an empty track.
 */
@Composable
private fun morphoSliderColors() = SliderDefaults.colors(
    activeTrackColor = MaterialTheme.colorScheme.primary,
    inactiveTrackColor = MaterialTheme.colorScheme.surfaceContainerHighest,
    thumbColor = MaterialTheme.colorScheme.primary,
    activeTickColor = MaterialTheme.colorScheme.onPrimary.copy(alpha = 0.4f),
    inactiveTickColor = MaterialTheme.colorScheme.outlineVariant,
)

@Composable
private fun SectionHeader(text: String) {
    Text(
        text = text,
        style = MaterialTheme.typography.titleSmall,
        color = MaterialTheme.colorScheme.primary,
        modifier = Modifier.padding(top = MorphoTheme.spacing.sm),
    )
}

@Composable
private fun SettingRow(
    title: String,
    summary: String? = null,
    onClick: (() -> Unit)? = null,
) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .heightIn(min = MorphoTheme.spacing.minTouchTarget)
            .then(if (onClick != null) Modifier.clickable(onClick = onClick) else Modifier)
            .padding(vertical = MorphoTheme.spacing.xs),
        verticalArrangement = Arrangement.spacedBy(MorphoTheme.spacing.xxs / 2),
    ) {
        Text(
            text = title,
            style = MaterialTheme.typography.bodyLarge,
            color = MaterialTheme.colorScheme.onSurface,
        )
        if (summary != null) {
            Text(
                text = summary,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

@Composable
private fun ToggleRow(
    title: String,
    summary: String,
    checked: Boolean,
    onCheckedChange: (Boolean) -> Unit,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .heightIn(min = MorphoTheme.spacing.minTouchTarget)
            .padding(vertical = MorphoTheme.spacing.xs),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(MorphoTheme.spacing.md),
    ) {
        Column(Modifier.weight(1f)) {
            Text(
                text = title,
                style = MaterialTheme.typography.bodyLarge,
                color = MaterialTheme.colorScheme.onSurface,
            )
            Text(
                text = summary,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        Switch(checked = checked, onCheckedChange = onCheckedChange)
    }
}

@ThemePreviews
@Composable
private fun SettingsPreview() {
    PreviewBox {
        SettingsContent(
            settings = MorphoSettings(dailyGoal = 50),
            wordCount = 24,
            contentVersion = "2026.08.26+demo0001",
            stubNoticeVisible = false,
            onDailyGoalChange = {},
            onSoundChange = {},
            onVolumeChange = {},
            onHapticsChange = {},
            onReducedMotionChange = {},
            onStubTapped = {},
        )
    }
}
