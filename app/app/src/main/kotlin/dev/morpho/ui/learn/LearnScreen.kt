package dev.morpho.ui.learn

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.background
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Close
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import dev.morpho.R
import dev.morpho.data.sound.SfxEvent
import dev.morpho.di.AppContainer
import dev.morpho.domain.model.LearnMode
import dev.morpho.ui.designsystem.component.AnswerFeedbackOverlay
import dev.morpho.ui.designsystem.component.DetailSheet
import dev.morpho.ui.designsystem.component.GroupProgressBar
import dev.morpho.ui.designsystem.component.GroupSegmentState
import dev.morpho.ui.designsystem.component.ImageOption
import dev.morpho.ui.designsystem.component.ModePips
import dev.morpho.ui.designsystem.component.PreviewBox
import dev.morpho.ui.designsystem.component.QuizImageDefGrid
import dev.morpho.ui.designsystem.component.QuizImageGrid
import dev.morpho.ui.designsystem.component.QuizTextOptions
import dev.morpho.ui.designsystem.component.SentenceCard
import dev.morpho.ui.designsystem.component.TextOption
import dev.morpho.ui.designsystem.component.ThemePreviews
import dev.morpho.ui.designsystem.component.WordHeader
import dev.morpho.ui.designsystem.motion.rememberSharedAxis
import dev.morpho.ui.designsystem.theme.MorphoTheme

/**
 * The learning screen: modes 1, 2 and 3 in one layout that swaps its stimulus and its
 * option grid, with the shared-axis X transition between questions.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun LearnScreen(
    container: AppContainer,
    onFinished: () -> Unit,
    onExit: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val viewModel: LearnViewModel = viewModel(factory = LearnViewModel.factory(container))
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
                            text = stringResource(R.string.learn_title),
                            style = MaterialTheme.typography.titleMedium,
                        )
                        Text(
                            text = stringResource(
                                R.string.learn_round,
                                state.round,
                                state.roundTotal,
                            ),
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
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
                actions = {
                    state.question?.let { ModePips(mode = it.mode.level) }
                    Spacer(Modifier.width(MorphoTheme.spacing.md))
                },
            )
        },
    ) { padding ->
        Box(Modifier.fillMaxSize()) {
            when {
                state.loading -> LoadingBox(Modifier.padding(padding))
                state.empty -> EmptyBox(Modifier.padding(padding))
                else -> QuestionBody(
                    state = state,
                    modifier = Modifier.padding(padding),
                    onSelect = viewModel::onOptionSelected,
                    onPlay = viewModel::onPlayAudio,
                )
            }

            AnswerFeedbackOverlay(
                signal = state.feedback,
                onCelebrationFinished = viewModel::onCelebrationFinished,
            )
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

@Composable
private fun QuestionBody(
    state: LearnUiState,
    onSelect: (Int) -> Unit,
    onPlay: (String?) -> Unit,
    modifier: Modifier = Modifier,
) {
    val spacing = MorphoTheme.spacing
    val axis = rememberSharedAxis()
    val question = state.question ?: return

    Column(
        modifier = modifier
            .fillMaxSize()
            .verticalScroll(rememberScrollState())
            .padding(horizontal = spacing.screenGutter),
        verticalArrangement = Arrangement.spacedBy(spacing.md),
    ) {
        GroupProgressBar(segments = state.groupSegments)

        AnimatedContent(
            targetState = question,
            transitionSpec = { axis.transform(forward = true) },
            label = "question",
            contentKey = { it.wordId to it.mode },
        ) { current ->
            Column(verticalArrangement = Arrangement.spacedBy(spacing.md)) {
                Stimulus(
                    question = current,
                    nowPlayingFile = state.nowPlayingFile,
                    onPlay = onPlay,
                )

                Text(
                    text = stringResource(
                        when (current.mode) {
                            LearnMode.SENTENCE_IMAGE -> R.string.learn_mode_1_prompt
                            LearnMode.WORD_IMAGE_DEF -> R.string.learn_mode_2_prompt
                            LearnMode.WORD_TEXT_DEF -> R.string.learn_mode_3_prompt
                        },
                    ),
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )

                when (current.mode) {
                    LearnMode.SENTENCE_IMAGE -> QuizImageGrid(
                        options = current.imageOptions,
                        onSelect = onSelect,
                        selectedIndex = state.selectedIndex,
                        correctIndex = current.correctIndex,
                        revealed = state.revealed,
                        enabled = !state.revealed || state.mustRetry,
                    )

                    LearnMode.WORD_IMAGE_DEF -> QuizImageDefGrid(
                        options = current.imageOptions,
                        onSelect = onSelect,
                        selectedIndex = state.selectedIndex,
                        correctIndex = current.correctIndex,
                        revealed = state.revealed,
                        enabled = !state.revealed || state.mustRetry,
                    )

                    LearnMode.WORD_TEXT_DEF -> QuizTextOptions(
                        options = current.textOptions,
                        onSelect = onSelect,
                        selectedIndex = state.selectedIndex,
                        correctIndex = current.correctIndex,
                        revealed = state.revealed,
                        enabled = !state.revealed || state.mustRetry,
                    )
                }
            }
        }

        // Wrong answer: the English definition appears as help and the user must pick
        // again before moving on (README Part 1, "选错 -> 显示英文释义辅助").
        AnimatedVisibility(
            visible = state.mustRetry,
            enter = fadeIn(tween(MorphoTheme.durations.fade)),
            exit = fadeOut(tween(MorphoTheme.durations.fade)),
        ) {
            RetryHint(definition = question.primaryDefinition)
        }

        Spacer(Modifier.height(spacing.md))
    }
}

@Composable
private fun Stimulus(
    question: QuestionUi,
    nowPlayingFile: String?,
    onPlay: (String?) -> Unit,
) {
    when (question.mode) {
        LearnMode.SENTENCE_IMAGE -> {
            val sentence = question.sentence
            if (sentence != null) {
                SentenceCard(
                    sentence = sentence,
                    highlight = question.highlight,
                    onPlayAudio = { onPlay(question.sentenceAudioFile) },
                    playing = nowPlayingFile == question.sentenceAudioFile,
                )
            }
        }

        LearnMode.WORD_IMAGE_DEF, LearnMode.WORD_TEXT_DEF -> WordHeader(
            word = question.word,
            phonetic = question.phonetic,
            onPlayAudio = { onPlay(question.wordAudioFile) },
            playing = nowPlayingFile == question.wordAudioFile,
        )
    }
}

@Composable
private fun RetryHint(definition: String) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .clip(MorphoTheme.radii.shapeMd)
            .background(MaterialTheme.colorScheme.errorContainer)
            .padding(MorphoTheme.spacing.md),
        verticalArrangement = Arrangement.spacedBy(MorphoTheme.spacing.xxs),
    ) {
        Text(
            text = stringResource(R.string.learn_wrong_hint),
            style = MaterialTheme.typography.labelLarge,
            color = MaterialTheme.colorScheme.onErrorContainer,
        )
        Text(
            text = definition,
            style = MorphoTheme.reading.definition,
            color = MaterialTheme.colorScheme.onErrorContainer,
        )
    }
}

@Composable
private fun LoadingBox(modifier: Modifier = Modifier) {
    Box(modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
        CircularProgressIndicator()
    }
}

@Composable
private fun EmptyBox(modifier: Modifier = Modifier) {
    Box(modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
        Text(
            text = stringResource(R.string.home_today_all_done),
            style = MaterialTheme.typography.bodyLarge,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            textAlign = TextAlign.Center,
        )
    }
}

// ------------------------------------------------------------------ previews

private val previewImageOptions = listOf(
    ImageOption(1, "img/a.webp", "kind and generous towards other people", "Option 1 of 4"),
    ImageOption(2, "img/b.webp", "wishing harm on other people", "Option 2 of 4"),
    ImageOption(3, "img/c.webp", "having two opposite feelings at once", "Option 3 of 4"),
    ImageOption(4, "img/d.webp", "gentle and causing no harm", "Option 4 of 4"),
)

private val previewQuestionMode1 = QuestionUi(
    wordId = 1,
    mode = LearnMode.SENTENCE_IMAGE,
    word = "benevolent",
    phonetic = "/bəˈnevələnt/",
    wordAudioFile = "audio/w.ogg",
    sentence = "A benevolent stranger paid for the whole table and left before anyone could thank him.",
    highlight = 2..11,
    sentenceAudioFile = "audio/s.ogg",
    imageOptions = previewImageOptions,
    correctIndex = 0,
    primaryDefinition = "kind and generous towards other people",
)

@ThemePreviews
@Composable
private fun LearnMode1Preview() {
    PreviewBox {
        QuestionBody(
            state = LearnUiState(
                loading = false,
                question = previewQuestionMode1,
                groupSegments = List(4) { GroupSegmentState.PASSED } +
                    listOf(GroupSegmentState.IN_PROGRESS) +
                    List(11) { GroupSegmentState.PENDING },
            ),
            onSelect = {},
            onPlay = {},
        )
    }
}

@ThemePreviews
@Composable
private fun LearnMode2Preview() {
    PreviewBox {
        QuestionBody(
            state = LearnUiState(
                loading = false,
                question = previewQuestionMode1.copy(mode = LearnMode.WORD_IMAGE_DEF),
                groupSegments = List(16) { GroupSegmentState.PENDING },
            ),
            onSelect = {},
            onPlay = {},
        )
    }
}

@ThemePreviews
@Composable
private fun LearnMode3WrongPreview() {
    PreviewBox {
        QuestionBody(
            state = LearnUiState(
                loading = false,
                question = previewQuestionMode1.copy(
                    mode = LearnMode.WORD_TEXT_DEF,
                    imageOptions = emptyList(),
                    textOptions = listOf(
                        TextOption(1, "kind and generous towards other people", "adj"),
                        TextOption(2, "wishing harm on other people", "adj"),
                        TextOption(3, "having two opposite feelings at once", "adj"),
                        TextOption(4, "gentle and causing no harm", "adj"),
                    ),
                ),
                selectedIndex = 2,
                revealed = true,
                mustRetry = true,
                groupSegments = List(16) { GroupSegmentState.PENDING },
            ),
            onSelect = {},
            onPlay = {},
        )
    }
}
