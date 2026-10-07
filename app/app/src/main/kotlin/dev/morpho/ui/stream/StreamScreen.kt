package dev.morpho.ui.stream

import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedContent
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Close
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.ProgressBarRangeInfo
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.progressBarRangeInfo
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.style.TextAlign
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import dev.morpho.R
import dev.morpho.di.AppContainer
import dev.morpho.domain.stream.StepKind
import dev.morpho.ui.designsystem.component.AudioChipButton
import dev.morpho.ui.designsystem.component.GlossedText
import dev.morpho.ui.designsystem.component.MorphoLoader
import dev.morpho.ui.designsystem.component.PrimaryButton
import dev.morpho.ui.designsystem.motion.pressMotion
import dev.morpho.ui.designsystem.motion.rememberSharedAxis
import dev.morpho.ui.designsystem.theme.MorphoTheme

/**
 * The stream (docs/contracts/stream.md): one frame for every step — pause, today's
 * progress bar and steps done / total on top, the step in the middle, its action at the
 * foot. When nothing is left, the done screen.
 */
@Composable
fun StreamScreen(
    container: AppContainer,
    onExit: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val viewModel: StreamViewModel = viewModel(factory = StreamViewModel.factory(container))
    val state by viewModel.state.collectAsStateWithLifecycle()
    val pause = {
        viewModel.onPause()
        onExit()
    }
    BackHandler(onBack = pause)

    Column(
        modifier = modifier
            .fillMaxSize()
            .background(MaterialTheme.colorScheme.background)
            .statusBarsPadding()
            .navigationBarsPadding()
            .imePadding(),
    ) {
        val summary = state.summary
        val view = state.view
        when {
            summary != null -> DoneView(
                summary = summary,
                onDone = pause,
                onMeetMore = viewModel::onMeetMore,
            )

            view == null -> Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { MorphoLoader() }

            else -> {
                StreamBar(done = state.done, total = state.total, onPause = pause)
                val axis = rememberSharedAxis()
                AnimatedContent(
                    targetState = view,
                    transitionSpec = { axis.transform(forward = true) },
                    contentKey = { it.serial },
                    label = "step",
                    modifier = Modifier.weight(1f),
                ) { shown ->
                    // The outgoing step keeps its last look; the step on screen follows every update.
                    val live = if (shown.serial == view.serial) view else shown
                    StepFrame(view = live, nowPlaying = state.nowPlaying, viewModel = viewModel)
                }
            }
        }
    }
}

/** Pause, today's progress and the steps done out of the estimated total. */
@Composable
private fun StreamBar(done: Int, total: Int, onPause: () -> Unit) {
    val spacing = MorphoTheme.spacing
    val label = stringResource(R.string.stream_progress, done, total)
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(start = spacing.xxs, end = spacing.sm),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(spacing.xs),
    ) {
        IconButton(onClick = onPause, modifier = Modifier.pressMotion()) {
            Icon(Icons.Rounded.Close, contentDescription = stringResource(R.string.action_pause))
        }
        val fraction = if (total == 0) 0f else done.toFloat() / total
        val streamLabel = stringResource(R.string.cd_stream_progress)
        Box(
            Modifier
                .weight(1f)
                .height(MorphoTheme.sizes.streamBarHeight)
                .clip(MorphoTheme.radii.shapeFull)
                .background(MorphoTheme.accents.ringTrack)
                .semantics {
                    contentDescription = streamLabel
                    progressBarRangeInfo = ProgressBarRangeInfo(done.toFloat(), 0f..total.coerceAtLeast(1).toFloat())
                },
        ) {
            Box(
                Modifier
                    .fillMaxWidth(fraction.coerceIn(0f, 1f))
                    .height(MorphoTheme.sizes.streamBarHeight)
                    .clip(MorphoTheme.radii.shapeFull)
                    .background(MaterialTheme.colorScheme.primary),
            )
        }
        Text(
            text = label,
            style = MaterialTheme.typography.labelMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            textAlign = TextAlign.End,
            modifier = Modifier.widthIn(min = spacing.xxl + spacing.xs),
        )
    }
}

/**
 * One step's frame: the step and its answer area (the piece bank, the word options, the
 * letter tiles) above the action at the foot. When they fit, the answer area sits at the
 * bottom, above the action; when a tall step (a large picture, a long definition) does not
 * fit, both scroll as one page, so the tray is never hidden under the bank.
 */
@Composable
private fun StepFrame(view: StepView, nowPlaying: String?, viewModel: StreamViewModel) {
    val spacing = MorphoTheme.spacing
    val verdict = view.verdict
    Column(Modifier.fillMaxSize()) {
        BoxWithConstraints(Modifier.weight(1f).fillMaxWidth()) {
            val viewport = maxHeight
            Column(
                modifier = Modifier
                    .fillMaxSize()
                    .verticalScroll(rememberScrollState())
                    .padding(horizontal = spacing.screenGutter),
            ) {
                Column(
                    modifier = Modifier.fillMaxWidth().heightIn(min = viewport),
                    verticalArrangement = Arrangement.SpaceBetween,
                ) {
                    Column(Modifier.padding(top = spacing.xxs, bottom = spacing.sm)) {
                        when {
                            verdict != null -> ReviewResultContent(view, verdict, nowPlaying, viewModel)
                            view.step.kind == StepKind.KNOW -> KnowContent(view, nowPlaying, viewModel)
                            else -> TaskContent(view, nowPlaying, viewModel)
                        }
                    }
                    if (verdict == null && view.task != null) {
                        Column { AnswerArea(view, viewModel) }
                    }
                }
            }
        }
        val action = Modifier.padding(horizontal = spacing.screenGutter, vertical = spacing.sm)
        when {
            verdict != null -> PrimaryButton(stringResource(R.string.action_continue), viewModel::onContinue, action)
            // A rebuild review goes on to spelling the word once its rebuild is right.
            view.step.kind == StepKind.REVIEW && view.task is RebuildState -> Column(action) {
                if (view.outcome == null) {
                    Text(
                        text = stringResource(R.string.stream_spell_next),
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        modifier = Modifier.padding(bottom = spacing.xs),
                    )
                }
                PrimaryButton(
                    text = stringResource(R.string.stream_spell_now),
                    onClick = viewModel::onContinue,
                    enabled = view.outcome != null,
                )
            }
            view.step.kind == StepKind.REVIEW -> Spacer(Modifier.height(spacing.sm))
            else -> PrimaryButton(
                text = stringResource(R.string.action_continue),
                onClick = viewModel::onContinue,
                enabled = view.step.kind == StepKind.KNOW || view.outcome != null,
                modifier = action,
            )
        }
    }
}

/** The lower half of a task: the bank of pieces, the four words, or the letter tiles. */
@Composable
private fun AnswerArea(view: StepView, viewModel: StreamViewModel) {
    when (val task = view.task) {
        is RebuildState -> {
            PieceBank(state = task, onPlace = viewModel::onPlacePiece)
            RebuildActions(
                state = task,
                onShowNextPiece = viewModel::onShowNextPiece,
                onShowAnswer = viewModel::onShowAnswer,
                onStartOver = viewModel::onStartOver,
            )
        }
        is FillState -> WordOptions(state = task, onPick = viewModel::onPickWord)
        is SpellState -> {
            LetterTiles(state = task, onPlace = viewModel::onPlaceLetter)
            SpellActions(
                state = task,
                onShowNextLetter = viewModel::onShowNextLetter,
                onShowWord = viewModel::onShowWord,
                onStartOver = viewModel::onStartOver,
            )
        }
        null -> Unit
    }
}

// ------------------------------------------------------------------ know

@Composable
private fun KnowContent(view: StepView, nowPlaying: String?, viewModel: StreamViewModel) {
    WordCard(
        word = view.word,
        mark = if (view.again) StageMark.LEARN else StageMark.NEW,
        label = if (view.again) {
            stringResource(R.string.stream_stage_again)
        } else {
            stringResource(R.string.stream_stage_new, view.unit)
        },
        note = view.note,
        playing = nowPlaying == view.word.word.wordAudioFile,
        onPlay = { viewModel.onPlayCard(view.word) },
    )
}

// ------------------------------------------------------------------ explain, spell, use, review task

@Composable
private fun TaskContent(view: StepView, nowPlaying: String?, viewModel: StreamViewModel) {
    val spacing = MorphoTheme.spacing
    val word = view.word
    val review = view.step.kind == StepKind.REVIEW
    Column(verticalArrangement = Arrangement.spacedBy(spacing.md)) {
        // A review, and spelling the word, keep the picture behind its button.
        if (review || view.step.kind == StepKind.SPELL) {
            if (view.pictureShown) {
                WordPicture(word, Modifier.fillMaxWidth().height(MorphoTheme.sizes.coverHeight))
            } else {
                CoveredPicture(Modifier.fillMaxWidth().height(MorphoTheme.sizes.coverHeight)) {
                    LinkButton(stringResource(R.string.stream_show_picture), viewModel::onShowPicture)
                }
            }
        }
        StageLabel(
            mark = if (review) StageMark.REVIEW else StageMark.LEARN,
            text = when (view.step.kind) {
                StepKind.EXPLAIN -> stringResource(R.string.stream_stage_explain)
                StepKind.SPELL -> stringResource(R.string.stream_stage_spell)
                StepKind.USE -> stringResource(R.string.stream_stage_use)
                else -> reviewLabel(view.reps)
            },
        )
        when (val task = view.task) {
            is RebuildState -> {
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(spacing.sm),
                ) {
                    if (view.step.kind == StepKind.EXPLAIN) {
                        WordPicture(
                            word,
                            Modifier.size(MorphoTheme.sizes.thumbnailWidth, MorphoTheme.sizes.thumbnailHeight),
                        )
                    }
                    AskTitle(word.word.word, Modifier.weight(1f))
                    if (review) {
                        AudioChipButton(
                            onClick = { viewModel.onPlayWord(word) },
                            playing = nowPlaying == word.word.wordAudioFile,
                            contentDescription = stringResource(R.string.cd_play_word, word.word.word),
                        )
                    }
                }
                RebuildTray(state = task, onReturn = viewModel::onReturnPiece)
            }

            is FillState -> {
                // A review hides the picture behind its button above; the word itself is
                // never shown here, since it is one of the options.
                if (!review) WidePicture(word)
                Text(
                    text = stringResource(R.string.stream_fill_title),
                    style = MorphoTheme.reading.prompt,
                    color = MaterialTheme.colorScheme.onSurface,
                )
                GapSentence(task)
                WrongMeanings(task)
            }

            is SpellState -> {
                Text(
                    text = stringResource(R.string.stream_spell_title),
                    style = MorphoTheme.reading.prompt,
                    color = MaterialTheme.colorScheme.onSurface,
                )
                MaskedDefinition(word)
                SpellBoard(state = task, onReturn = viewModel::onReturnLetter)
            }

            null -> Unit
        }
    }
}

/** "What does *word* mean?", the word over a copper rule. */
@Composable
private fun AskTitle(word: String, modifier: Modifier = Modifier) {
    val copper = MorphoTheme.accents.motifActive
    val text = stringResource(R.string.stream_explain_title, word)
    val start = text.indexOf(word)
    GlossedText(
        text = AnnotatedString(text),
        style = MorphoTheme.reading.prompt,
        color = MaterialTheme.colorScheme.onSurface,
        underlines = if (start < 0) emptyList() else listOf(start until start + word.length),
        underlineColor = copper,
        modifier = modifier,
    )
}

/** "Review · 4th time", where 4 is the card's review count before this review; "Review" without one. */
@Composable
internal fun reviewLabel(reps: Int): String =
    if (reps > 0) stringResource(R.string.stream_stage_review, ordinal(reps)) else stringResource(R.string.stream_stage_review_plain)

/** 1 -> "1st", 2 -> "2nd", 11 -> "11th". */
internal fun ordinal(n: Int): String {
    val suffix = if (n % 100 in 11..13) {
        "th"
    } else {
        when (n % 10) {
            1 -> "st"
            2 -> "nd"
            3 -> "rd"
            else -> "th"
        }
    }
    return "$n$suffix"
}
