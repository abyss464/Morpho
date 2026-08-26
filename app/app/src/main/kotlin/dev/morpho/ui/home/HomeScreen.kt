package dev.morpho.ui.home

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.AutoAwesome
import androidx.compose.material.icons.rounded.MenuBook
import androidx.compose.material.icons.rounded.Refresh
import androidx.compose.material.icons.rounded.Settings
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ElevatedCard
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import dev.morpho.R
import dev.morpho.data.sound.SfxEvent
import dev.morpho.di.AppContainer
import dev.morpho.di.StartupReport
import dev.morpho.ui.designsystem.component.ProgressRing
import dev.morpho.ui.designsystem.component.StatTile
import dev.morpho.ui.designsystem.component.StreakBadge
import dev.morpho.ui.designsystem.component.ThemePreviews
import dev.morpho.ui.designsystem.component.PreviewBox
import dev.morpho.ui.designsystem.theme.MorphoTheme
import dev.morpho.domain.progress.OverallProgress
import dev.morpho.domain.progress.TodayProgress

/**
 * Home: overall progress ring, today's task card, streak, and the single call to
 * action that starts whichever queue is due first (reviews before new words,
 * README Part 1, "每日流程").
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun HomeScreen(
    container: AppContainer,
    startup: StartupReport,
    onStartLearning: () -> Unit,
    onStartReview: () -> Unit,
    onOpenSettings: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val viewModel: HomeViewModel = viewModel(factory = HomeViewModel.factory(container))
    val state by viewModel.state.collectAsStateWithLifecycle()

    LaunchedEffect(Unit) { viewModel.refresh() }

    Scaffold(
        modifier = modifier.fillMaxSize(),
        topBar = {
            TopAppBar(
                title = { Text(stringResource(R.string.home_title)) },
                actions = {
                    IconButton(onClick = {
                        container.playSfx(SfxEvent.TAP)
                        onOpenSettings()
                    }) {
                        Icon(
                            Icons.Rounded.Settings,
                            contentDescription = stringResource(R.string.action_settings),
                        )
                    }
                },
            )
        },
    ) { padding ->
        HomeContent(
            state = state,
            wordCount = startup.wordCount,
            modifier = Modifier
                .fillMaxSize()
                .padding(padding),
            onStartLearning = {
                container.playSfx(SfxEvent.TAP)
                onStartLearning()
            },
            onStartReview = {
                container.playSfx(SfxEvent.TAP)
                onStartReview()
            },
        )
    }
}

@Composable
private fun HomeContent(
    state: HomeUiState,
    wordCount: Int,
    onStartLearning: () -> Unit,
    onStartReview: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val spacing = MorphoTheme.spacing
    Column(
        modifier = modifier
            .verticalScroll(rememberScrollState())
            .padding(horizontal = spacing.screenGutter)
            .padding(bottom = spacing.xxl),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(spacing.xl),
    ) {
        Spacer(Modifier.height(spacing.xs))

        ProgressRing(
            progress = state.overall.fraction,
            centerLabel = state.overall.learnedWords.toString(),
            centerCaption = stringResource(
                R.string.home_ring_caption,
                formatCount(state.overall.totalWords),
            ),
            accessibilityLabel = stringResource(
                R.string.cd_progress_ring,
                state.overall.learnedWords,
                state.overall.totalWords,
            ),
        )

        if (state.streakDays > 0) {
            StreakBadge(days = state.streakDays)
        }

        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(spacing.sm),
        ) {
            StatTile(
                value = state.overall.learnedWords.toString(),
                label = stringResource(R.string.home_stat_learned),
                icon = Icons.Rounded.AutoAwesome,
                modifier = Modifier.weight(1f),
            )
            StatTile(
                value = state.overall.inFlightWords.toString(),
                label = stringResource(R.string.home_stat_in_progress),
                icon = Icons.Rounded.MenuBook,
                modifier = Modifier.weight(1f),
            )
            StatTile(
                value = formatCount(state.overall.remainingWords),
                label = stringResource(R.string.home_stat_remaining),
                modifier = Modifier.weight(1f),
            )
        }

        TodayCard(
            today = state.today,
            hasContent = wordCount > 0,
            onStartLearning = onStartLearning,
            onStartReview = onStartReview,
        )
    }
}

@Composable
private fun TodayCard(
    today: TodayProgress,
    hasContent: Boolean,
    onStartLearning: () -> Unit,
    onStartReview: () -> Unit,
) {
    val spacing = MorphoTheme.spacing
    ElevatedCard(
        modifier = Modifier.fillMaxWidth(),
        shape = MorphoTheme.radii.shapeLg,
        colors = CardDefaults.elevatedCardColors(
            containerColor = MaterialTheme.colorScheme.surfaceContainerLow,
        ),
        elevation = CardDefaults.elevatedCardElevation(
            defaultElevation = MorphoTheme.elevations.card,
        ),
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(spacing.xl),
            verticalArrangement = Arrangement.spacedBy(spacing.md),
        ) {
            Text(
                text = stringResource(R.string.home_today_title),
                style = MaterialTheme.typography.titleMedium,
                color = MaterialTheme.colorScheme.onSurface,
            )

            if (!hasContent) {
                Text(
                    text = stringResource(R.string.home_empty),
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                return@Column
            }

            Text(
                text = stringResource(
                    R.string.home_today_goal_progress,
                    today.newLearned,
                    today.dailyGoal,
                ),
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            LinearProgressIndicator(
                progress = { today.fraction },
                modifier = Modifier
                    .fillMaxWidth()
                    .heightIn(min = MorphoTheme.sizes.groupBarHeight),
                // The M3 default track resolves to the secondary container, which is
                // teal in this brand and reads as a second value rather than an
                // empty track. Use the neutral ring track instead.
                trackColor = MorphoTheme.accents.ringTrack,
                strokeCap = androidx.compose.ui.graphics.StrokeCap.Round,
            )

            if (today.dueReviews > 0) {
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(spacing.xs),
                ) {
                    Icon(
                        Icons.Rounded.Refresh,
                        contentDescription = null,
                        tint = MaterialTheme.colorScheme.secondary,
                    )
                    Text(
                        text = stringResource(R.string.home_today_reviews, today.dueReviews),
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurface,
                    )
                }
            }

            if (!today.hasWork) {
                Text(
                    text = stringResource(R.string.home_today_all_done),
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.secondary,
                )
            }

            Spacer(Modifier.height(spacing.xxs))

            // Reviews come first when anything is due; the CTA reflects that order.
            if (today.dueReviews > 0) {
                Button(
                    onClick = onStartReview,
                    modifier = Modifier
                        .fillMaxWidth()
                        .heightIn(min = spacing.minTouchTarget),
                    shape = MorphoTheme.radii.shapeLg,
                    colors = ButtonDefaults.buttonColors(),
                ) {
                    Text(stringResource(R.string.home_start_review))
                }
                OutlinedButton(
                    onClick = onStartLearning,
                    modifier = Modifier
                        .fillMaxWidth()
                        .heightIn(min = spacing.minTouchTarget),
                    shape = MorphoTheme.radii.shapeLg,
                ) {
                    Text(stringResource(R.string.home_start_learning))
                }
            } else {
                Button(
                    onClick = onStartLearning,
                    modifier = Modifier
                        .fillMaxWidth()
                        .heightIn(min = spacing.minTouchTarget),
                    shape = MorphoTheme.radii.shapeLg,
                    enabled = !today.goalMet || today.remainingNew > 0,
                ) {
                    Text(stringResource(R.string.home_start_learning))
                }
            }
        }
    }
}

/** 5500 -> "5,500". Kept local: it is presentation, not domain. */
internal fun formatCount(value: Int): String =
    java.text.NumberFormat.getIntegerInstance().format(value)

@ThemePreviews
@Composable
private fun HomeContentPreview() {
    PreviewBox {
        Box(Modifier.fillMaxWidth()) {
            HomeContent(
                state = HomeUiState(
                    loading = false,
                    overall = OverallProgress(totalWords = 5500, learnedWords = 2090, inFlightWords = 17),
                    today = TodayProgress(
                        newLearned = 20,
                        dailyGoal = 50,
                        reviewed = 8,
                        dueReviews = 12,
                        correctCount = 23,
                        answerCount = 25,
                    ),
                    streakDays = 12,
                    contentVersion = "2026.08.26+demo0001",
                ),
                wordCount = 5500,
                onStartLearning = {},
                onStartReview = {},
            )
        }
    }
}
