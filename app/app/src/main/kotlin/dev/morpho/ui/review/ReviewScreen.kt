package dev.morpho.ui.review

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Close
import androidx.compose.material3.Button
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ElevatedCard
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
import dev.morpho.domain.review.ReviewQuestionType
import dev.morpho.ui.designsystem.component.AnswerFeedbackOverlay
import dev.morpho.ui.designsystem.component.AudioChipButton
import dev.morpho.ui.designsystem.component.DetailSheet
import dev.morpho.ui.designsystem.component.PosChip
import dev.morpho.ui.designsystem.component.QuizLayout
import dev.morpho.ui.designsystem.component.QuizTextOptions
import dev.morpho.ui.designsystem.component.ScreenPreviewBox
import dev.morpho.ui.designsystem.component.ScreenPreviews
import dev.morpho.ui.designsystem.component.SpellInput
import dev.morpho.ui.designsystem.component.SpellState
import dev.morpho.ui.designsystem.component.TextOption
import dev.morpho.ui.designsystem.theme.MorphoTheme

/**
 * Review: definition-to-word for words the user rarely forgets, listening-spell for the
 * ones they keep losing.
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
                ) { CircularProgressIndicator() }

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
                    onSpellingChanged = viewModel::onSpellingChanged,
                    onSubmit = viewModel::onSpellingSubmitted,
                    onReplay = viewModel::onReplayWord,
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
 * Same skeleton as the learning screen, for the same reason: whatever the user has to
 * touch — the four word cards, or the spell input and its check button — belongs at the
 * bottom of the viewport, and the thing they read sits above it.
 */
@Composable
private fun ReviewBody(
    state: ReviewUiState,
    onSelect: (Int) -> Unit,
    onSpellingChanged: (String) -> Unit,
    onSubmit: () -> Unit,
    onReplay: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val spacing = MorphoTheme.spacing
    val question = state.question ?: return

    QuizLayout(
        modifier = modifier,
        header = {
            LinearProgressIndicator(
                progress = { if (state.total == 0) 0f else state.index.toFloat() / state.total },
                modifier = Modifier
                    .fillMaxWidth()
                    .heightIn(min = MorphoTheme.sizes.groupBarHeight),
                // Neutral track: the M3 default resolves to the teal secondary container.
                trackColor = MorphoTheme.accents.ringTrack,
                strokeCap = androidx.compose.ui.graphics.StrokeCap.Round,
            )
        },
        prompt = {
            when (question.type) {
                ReviewQuestionType.DEFINITION_TO_WORD -> {
                    Text(
                        text = stringResource(R.string.review_definition_prompt),
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                    DefinitionPrompt(pos = question.pos, definition = question.definition)
                }

                ReviewQuestionType.LISTENING_SPELL -> {
                    Text(
                        text = stringResource(R.string.review_spell_prompt),
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                    Column(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalAlignment = Alignment.CenterHorizontally,
                        verticalArrangement = Arrangement.spacedBy(spacing.xs),
                    ) {
                        AudioChipButton(
                            onClick = onReplay,
                            playing = state.nowPlayingFile == question.wordAudioFile,
                            contentDescription = stringResource(R.string.action_replay_audio),
                        )
                        Text(
                            text = stringResource(R.string.review_spell_hint),
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                }
            }
        },
        // The miss verdict used to hang below the check button, which pushed the button
        // itself down the moment it appeared. Above the answer zone it changes nothing
        // the thumb is aiming at.
        banner = {
            if (state.revealed && state.spellState == SpellState.WRONG) {
                Text(
                    text = stringResource(R.string.review_wrong, question.word),
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.error,
                )
            }
        },
        answers = {
            when (question.type) {
                ReviewQuestionType.DEFINITION_TO_WORD -> QuizTextOptions(
                    options = question.wordOptions,
                    onSelect = onSelect,
                    selectedIndex = state.selectedIndex,
                    correctIndex = question.correctIndex,
                    revealed = state.revealed,
                    enabled = !state.revealed,
                )

                ReviewQuestionType.LISTENING_SPELL -> Column(
                    modifier = Modifier.fillMaxWidth(),
                    verticalArrangement = Arrangement.spacedBy(spacing.md),
                ) {
                    SpellInput(
                        target = question.word,
                        value = state.spelling,
                        onValueChange = onSpellingChanged,
                        onSubmit = onSubmit,
                        state = state.spellState,
                        enabled = !state.revealed,
                        revealAnswer = state.revealed,
                    )
                    Button(
                        onClick = onSubmit,
                        modifier = Modifier
                            .fillMaxWidth()
                            .heightIn(min = spacing.minTouchTarget),
                        shape = MorphoTheme.radii.shapeLg,
                        enabled = !state.revealed && state.spelling.isNotBlank(),
                    ) {
                        Text(stringResource(R.string.action_check))
                    }
                }
            }
        },
    )
}

@Composable
private fun DefinitionPrompt(pos: String, definition: String) {
    ElevatedCard(
        modifier = Modifier.fillMaxWidth(),
        shape = MorphoTheme.radii.shapeMd,
        colors = CardDefaults.elevatedCardColors(
            containerColor = MaterialTheme.colorScheme.surfaceContainerLow,
        ),
        elevation = CardDefaults.elevatedCardElevation(
            defaultElevation = MorphoTheme.elevations.raised,
        ),
    ) {
        Column(
            Modifier
                .fillMaxWidth()
                .padding(MorphoTheme.spacing.lg),
            verticalArrangement = Arrangement.spacedBy(MorphoTheme.spacing.xs),
        ) {
            PosChip(pos)
            Text(
                text = definition,
                style = MorphoTheme.reading.sentence,
                color = MaterialTheme.colorScheme.onSurface,
            )
        }
    }
}

// ------------------------------------------------------------------ previews

private val previewDefinitionQuestion = ReviewQuestionUi(
    wordId = 1,
    type = ReviewQuestionType.DEFINITION_TO_WORD,
    word = "adapt",
    phonetic = "/əˈdæpt/",
    wordAudioFile = "audio/w.ogg",
    definition = "to change something so it works in a new situation",
    definitionAudioFile = "audio/d.ogg",
    pos = "verb",
    wordOptions = listOf(
        TextOption(1, "adopt", serif = false),
        TextOption(2, "adapt", serif = false),
        TextOption(3, "adept", serif = false),
        TextOption(4, "adroit", serif = false),
    ),
    correctIndex = 1,
)

@ScreenPreviews
@Composable
private fun ReviewDefinitionPreview() {
    ScreenPreviewBox {
        ReviewBody(
            state = ReviewUiState(
                loading = false,
                question = previewDefinitionQuestion,
                index = 3,
                total = 12,
            ),
            onSelect = {},
            onSpellingChanged = {},
            onSubmit = {},
            onReplay = {},
        )
    }
}

@ScreenPreviews
@Composable
private fun ReviewSpellPreview() {
    ScreenPreviewBox {
        ReviewBody(
            state = ReviewUiState(
                loading = false,
                question = previewDefinitionQuestion.copy(
                    type = ReviewQuestionType.LISTENING_SPELL,
                    word = "benevolent",
                ),
                index = 5,
                total = 12,
                spelling = "benev",
            ),
            onSelect = {},
            onSpellingChanged = {},
            onSubmit = {},
            onReplay = {},
        )
    }
}
