package dev.morpho.ui.settings

import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
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
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilterChip
import androidx.compose.material3.FilterChipDefaults
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Slider
import androidx.compose.material3.SliderDefaults
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import dev.morpho.BuildConfig
import dev.morpho.R
import dev.morpho.data.backup.BackupSummary
import dev.morpho.data.backup.BackupVerdict
import dev.morpho.data.backup.ProgressBackup
import dev.morpho.data.repository.MorphoSettings
import dev.morpho.data.repository.SettingsRepository
import dev.morpho.di.AppContainer
import dev.morpho.di.StartupReport
import dev.morpho.domain.model.ActivityChartStyle
import dev.morpho.domain.model.ThemeMode
import dev.morpho.ui.designsystem.component.PreviewBox
import dev.morpho.ui.designsystem.component.ThemePreviews
import dev.morpho.ui.designsystem.motion.pressMotion
import dev.morpho.ui.designsystem.theme.MorphoTheme

/**
 * Settings: daily goal, the sound and haptics toggles the design contract requires, and
 * the progress export/import flow over the Storage Access Framework.
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
    val backup by viewModel.backup.collectAsStateWithLifecycle()

    val exportLauncher = rememberLauncherForActivityResult(
        ActivityResultContracts.CreateDocument(ProgressBackup.MIME_TYPE),
    ) { viewModel.onExportTarget(it) }

    // Any type: pickers disagree on what to call a SQLite file, and every candidate is
    // validated before a single byte is swapped in.
    val importLauncher = rememberLauncherForActivityResult(
        ActivityResultContracts.OpenDocument(),
    ) { viewModel.onImportSource(it) }

    Scaffold(
        modifier = modifier.fillMaxSize(),
        topBar = {
            TopAppBar(
                title = { Text(stringResource(R.string.settings_title)) },
                navigationIcon = {
                    IconButton(
                        onClick = {
                            container.tap()
                            onBack()
                        },
                        modifier = Modifier.pressMotion(),
                    ) {
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
            backupStatus = backupStatusText(backup),
            modifier = Modifier
                .fillMaxSize()
                .padding(padding),
            onDailyGoalChange = viewModel::setDailyGoal,
            onThemeModeChange = viewModel::setThemeMode,
            onActivityChartStyleChange = viewModel::setActivityChartStyle,
            onSoundChange = viewModel::setSoundEnabled,
            onVolumeChange = viewModel::setSfxVolume,
            onHapticsChange = viewModel::setHapticsEnabled,
            onReducedMotionChange = viewModel::setReducedMotion,
            onExport = {
                viewModel.onTap()
                viewModel.dismissMessage()
                exportLauncher.launch(viewModel.suggestedExportName())
            },
            onImport = {
                viewModel.onTap()
                viewModel.dismissMessage()
                importLauncher.launch(arrayOf("*/*"))
            },
        )
    }

    backup.pending?.let { staged ->
        ImportConfirmDialog(
            summary = staged.summary,
            onConfirm = viewModel::confirmImport,
            onDismiss = viewModel::cancelImport,
        )
    }
}

/**
 * The one destructive action in the app, so it says plainly what disappears and offers
 * a decline that names the thing being protected rather than a bare "Cancel".
 */
@Composable
private fun ImportConfirmDialog(
    summary: BackupSummary?,
    onConfirm: () -> Unit,
    onDismiss: () -> Unit,
) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(stringResource(R.string.backup_import_title)) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(MorphoTheme.spacing.sm)) {
                Text(stringResource(R.string.backup_import_body))
                if (summary != null) {
                    Text(
                        stringResource(
                            R.string.backup_import_contents,
                            summary.cardsScheduled,
                            summary.daysRecorded,
                        ),
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
                Text(
                    stringResource(R.string.backup_import_restart_note),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        },
        confirmButton = {
            TextButton(onClick = onConfirm, modifier = Modifier.pressMotion()) {
                Text(
                    stringResource(R.string.backup_import_confirm),
                    color = MaterialTheme.colorScheme.error,
                )
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss, modifier = Modifier.pressMotion()) {
                Text(stringResource(R.string.backup_import_cancel))
            }
        },
    )
}

/** Turns the view model's backup state into one line of copy under the data section. */
@Composable
private fun backupStatusText(state: BackupUiState): String? = when {
    state.busy -> stringResource(R.string.backup_working)
    state.message is BackupMessage.Exported ->
        stringResource(R.string.backup_export_done, formatBytes(state.message.bytes))
    state.message is BackupMessage.ExportFailed -> stringResource(R.string.backup_export_failed)
    state.message is BackupMessage.ImportFailed -> stringResource(R.string.backup_import_failed)
    state.message is BackupMessage.Rejected -> rejectionText(state.message.verdict)
    else -> null
}

@Composable
private fun rejectionText(verdict: BackupVerdict): String = when (verdict) {
    is BackupVerdict.NotSqlite -> stringResource(R.string.backup_reject_not_sqlite)
    is BackupVerdict.MissingTables ->
        stringResource(R.string.backup_reject_missing_tables, verdict.missing.joinToString(", "))
    is BackupVerdict.MissingSchemaVersion -> stringResource(R.string.backup_reject_no_version)
    is BackupVerdict.UnreadableSchemaVersion ->
        stringResource(R.string.backup_reject_bad_version, verdict.raw)
    is BackupVerdict.NewerSchema ->
        stringResource(R.string.backup_reject_newer, verdict.found, verdict.supported)
    is BackupVerdict.Ok -> ""
}

private fun formatBytes(bytes: Long): String = when {
    bytes >= 1024 * 1024 -> "%.1f MB".format(bytes / (1024.0 * 1024.0))
    bytes >= 1024 -> "%.0f KB".format(bytes / 1024.0)
    else -> "$bytes B"
}

@Composable
private fun SettingsContent(
    settings: MorphoSettings,
    wordCount: Int,
    contentVersion: String?,
    backupStatus: String?,
    onDailyGoalChange: (Int) -> Unit,
    onThemeModeChange: (ThemeMode) -> Unit,
    onActivityChartStyleChange: (ActivityChartStyle) -> Unit,
    onSoundChange: (Boolean) -> Unit,
    onVolumeChange: (Float) -> Unit,
    onHapticsChange: (Boolean) -> Unit,
    onReducedMotionChange: (Boolean?) -> Unit,
    onExport: () -> Unit,
    onImport: () -> Unit,
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
        SectionHeader(stringResource(R.string.settings_section_display))

        SettingRow(
            title = stringResource(R.string.settings_theme),
            summary = stringResource(R.string.settings_theme_summary),
        )
        Row(
            horizontalArrangement = Arrangement.spacedBy(spacing.xs),
        ) {
            ChoiceChip(
                label = stringResource(R.string.settings_theme_system),
                selected = settings.themeMode == ThemeMode.SYSTEM,
                onClick = { onThemeModeChange(ThemeMode.SYSTEM) },
            )
            ChoiceChip(
                label = stringResource(R.string.settings_theme_light),
                selected = settings.themeMode == ThemeMode.LIGHT,
                onClick = { onThemeModeChange(ThemeMode.LIGHT) },
            )
            ChoiceChip(
                label = stringResource(R.string.settings_theme_dark),
                selected = settings.themeMode == ThemeMode.DARK,
                onClick = { onThemeModeChange(ThemeMode.DARK) },
            )
        }

        SettingRow(title = stringResource(R.string.settings_activity_chart))
        Row(
            horizontalArrangement = Arrangement.spacedBy(spacing.xs),
        ) {
            ChoiceChip(
                label = stringResource(R.string.settings_activity_chart_bar),
                selected = settings.activityChartStyle == ActivityChartStyle.BAR,
                onClick = { onActivityChartStyleChange(ActivityChartStyle.BAR) },
            )
            ChoiceChip(
                label = stringResource(R.string.settings_activity_chart_heatmap),
                selected = settings.activityChartStyle == ActivityChartStyle.HEATMAP,
                onClick = { onActivityChartStyleChange(ActivityChartStyle.HEATMAP) },
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
            onClick = onExport,
        )
        SettingRow(
            title = stringResource(R.string.settings_import),
            summary = stringResource(R.string.settings_import_summary),
            onClick = onImport,
        )
        if (backupStatus != null) {
            Text(
                text = backupStatus,
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
            .then(if (onClick != null) Modifier.pressMotion().clickable(onClick = onClick) else Modifier)
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

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun ChoiceChip(
    label: String,
    selected: Boolean,
    onClick: () -> Unit,
) {
    FilterChip(
        selected = selected,
        onClick = onClick,
        modifier = Modifier.pressMotion(),
        label = { Text(label) },
        colors = FilterChipDefaults.filterChipColors(
            selectedContainerColor = MaterialTheme.colorScheme.tertiaryContainer,
            selectedLabelColor = MaterialTheme.colorScheme.onTertiaryContainer,
        ),
    )
}

@ThemePreviews
@Composable
private fun SettingsPreview() {
    PreviewBox {
        SettingsContent(
            settings = MorphoSettings(dailyGoal = 50),
            wordCount = 4_253,
            contentVersion = "2026.08.26+ff7fd531",
            backupStatus = null,
            onDailyGoalChange = {},
            onThemeModeChange = {},
            onActivityChartStyleChange = {},
            onSoundChange = {},
            onVolumeChange = {},
            onHapticsChange = {},
            onReducedMotionChange = {},
            onExport = {},
            onImport = {},
        )
    }
}

@ThemePreviews
@Composable
private fun ImportConfirmDialogPreview() {
    PreviewBox {
        ImportConfirmDialog(
            summary = BackupSummary(cardsScheduled = 640, daysRecorded = 47),
            onConfirm = {},
            onDismiss = {},
        )
    }
}
