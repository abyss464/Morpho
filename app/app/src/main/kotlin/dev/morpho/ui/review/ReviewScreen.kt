package dev.morpho.ui.review

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Close
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import dev.morpho.R
import dev.morpho.data.sound.SfxEvent
import dev.morpho.di.AppContainer
import dev.morpho.ui.designsystem.component.AnswerFeedbackOverlay
import dev.morpho.ui.designsystem.component.DetailSheet
import dev.morpho.ui.designsystem.component.ImageOption
import dev.morpho.ui.designsystem.component.MorphoLoader
import dev.morpho.ui.designsystem.component.QuizImageDefGrid
import dev.morpho.ui.designsystem.component.QuizLayout
import dev.morpho.ui.designsystem.component.RetryHelpCard
import dev.morpho.ui.designsystem.component.ScreenPreviewBox
import dev.morpho.ui.designsystem.component.ScreenPreviews
import dev.morpho.ui.designsystem.component.SenseDetail
import dev.morpho.ui.designsystem.component.WordHeader
import dev.morpho.ui.designsystem.theme.MorphoTheme

/**
 * Review: unified mode-2 visual for every due card — word + phonetic on top, 2x2
 * image+definition grid in the thumb zone.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ReviewScreen(
    container: AppContainer,
    onFinished: () -> Unit,
    onExit: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val viewModel: ReviewViewModel = viewModel(factory = ReviewViewModel.factory(container))
    val state by viewModel.state.collectAsStateWithLifecycle()

    LaunchedEffect(state.finished) {
        if (state.finished) onFinished()
    }

    Scaffold(
        modifier = modifier.fillMaxSize(),
        topBar = {
            TopAppBar(
                title = {
                    Column {
                        Text(
                            text = stringResource(R.string.review_title),
                            style = MaterialTheme.typography.titleMedium,
                        )
                        if (state.total > 0) {
                            Text(
                                text = stringResource(
                                    R.string.review_progress,
                                    state.index + 1,
                                    state.total,
                                ),
                                style = MaterialTheme.typography.bodySmall,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                        }
                    }
                },
                navigationIcon = {
                    IconButton(onClick = {
                        container.playSfx(SfxEvent.TAP)
                        viewModel.onExit()
                        onExit()
                    }) {
                        Icon(
                            Icons.Rounded.Close,
                            contentDescription = stringResource(R.string.action_end_session),
                        )
                    }
                },
            )
        },
    ) { padding ->
        Box(Modifier.fillMaxSize()) {
            when {
                state.loading -> Box(
                    Modifier.fillMaxSize().padding(padding),
                    contentAlignment = Alignment.Center,
                ) { MorphoLoader() }

                state.empty -> Box(
                    Modifier.fillMaxSize().padding(padding),
                    contentAlignment = Alignment.Center,
                ) {
                    Text(
                        text = stringResource(R.string.review_none_due),
                        style = MaterialTheme.typography.bodyLarge,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        textAlign = TextAlign.Center,
                    )
                }

                else -> ReviewBody(
                    state = state,
                    modifier = Modifier.padding(padding),
                    onSelect = viewModel::onOptionSelected,
                    onPlayWord = viewModel::onReplayWord,
                )
            }

            AnswerFeedbackOverlay(signal = state.feedback, onCelebrationFinished = {})
        }
    }

    state.detail?.let { detail ->
        DetailSheet(
            detail = detail,
            onDismiss = viewModel::onDetailDismissed,
            onPlay = viewModel::onPlayAudio,
            playingFile = state.nowPlayingFile,
            continueLabel = stringResource(R.string.action_continue),
            onContinue = viewModel::onDetailDismissed,
        )
    }
}

/**
 * Mode-2 layout reused from learning: WordHeader stimulus on top, 2x2 image+definition
 * grid in the bottom-anchored answer zone. On a wrong tap the retry help card appears
 * between the prompt and the grid (same as learning wrong-answer flow).
 */
@Composable
private fun ReviewBody(
    state: ReviewUiState,
    onSelect: (Int) -> Unit,
    onPlayWord: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val question = state.question ?: return

    QuizLayout(
        modifier = modifier,
        header = {
            LinearProgressIndicator(
                progress = { if (state.total == 0) 0f else state.index.toFloat() / state.total },
                modifier = Modifier
                    .fillMaxWidth()
                    .heightIn(min = MorphoTheme.sizes.groupBarHeight),
                trackColor = MorphoTheme.accents.ringTrack,
                strokeCap = androidx.compose.ui.graphics.StrokeCap.Round,
            )
        },
        prompt = {
            WordHeader(
                word = question.word,
                phonetic = question.phonetic,
                onPlayAudio = onPlayWord,
                playing = state.nowPlayingFile == question.wordAudioFile,
            )

            Text(
                text = stringResource(R.string.review_prompt),
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        },
        banner = {
            AnimatedVisibility(
                visible = state.mustRetry,
                enter = fadeIn(tween(MorphoTheme.durations.fade)),
                exit = fadeOut(tween(MorphoTheme.durations.fade)),
            ) {
                RetryHelpCard(
                    hint = stringResource(R.string.learn_wrong_hint),
                    word = question.word,
                    phonetic = question.phonetic,
                    senses = question.senses,
                    onPlayWord = onPlayWord,
                    playing = state.nowPlayingFile == question.wordAudioFile,
                )
            }
        },
        answers = { space ->
            QuizImageDefGrid(
                options = question.imageOptions,
                onSelect = onSelect,
                selectedIndex = state.selectedIndex,
                correctIndex = question.correctIndex,
                revealed = state.revealed,
                enabled = !state.revealed || state.mustRetry,
                space = space,
            )
        },
    )
}

// ------------------------------------------------------------------ previews

private val previewImageOptions = listOf(
    ImageOption(1, "img/a.webp", "kind and generous towards other people", "Option 1 of 4"),
    ImageOption(2, "img/b.webp", "wishing harm on other people", "Option 2 of 4"),
    ImageOption(3, "img/c.webp", "having two opposite feelings at once", "Option 3 of 4"),
    ImageOption(4, "img/d.webp", "gentle and causing no harm", "Option 4 of 4"),
)

private val previewSenses = listOf(
    SenseDetail("adj", "kind and generous towards other people", true, "audio/d1.ogg"),
    SenseDetail("adj", "wishing to do good and to help", false, "audio/d2.ogg"),
)

private val previewQuestion = ReviewQuestionUi(
    wordId = 1,
    word = "benevolent",
    phonetic = "/bəˈnevələnt/",
    wordAudioFile = "audio/w.ogg",
    imageOptions = previewImageOptions,
    correctIndex = 0,
    senses = previewSenses,
)

@ScreenPreviews
@Composable
private fun ReviewIdlePreview() {
    ScreenPreviewBox {
        ReviewBody(
            state = ReviewUiState(
                loading = false,
                question = previewQuestion,
                index = 3,
                total = 12,
            ),
            onSelect = {},
            onPlayWord = {},
        )
    }
}

@ScreenPreviews
@Composable
private fun ReviewWrongRetryPreview() {
    ScreenPreviewBox {
        ReviewBody(
            state = ReviewUiState(
                loading = false,
                question = previewQuestion,
                index = 5,
                total = 12,
                selectedIndex = 2,
                revealed = true,
                mustRetry = true,
            ),
            onSelect = {},
            onPlayWord = {},
        )
    }
}
