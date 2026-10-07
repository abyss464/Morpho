package dev.morpho.ui.stream

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.VerticalDivider
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.unit.dp
import dev.morpho.R
import dev.morpho.domain.review.Grade
import dev.morpho.domain.stream.Outcome
import dev.morpho.domain.stream.ReviewTask
import dev.morpho.ui.designsystem.motion.pressMotion
import dev.morpho.ui.designsystem.theme.MorphoTheme

/**
 * A solved review (docs/contracts/stream.md §2, §6): the full card with the rating the
 * task earned and its next interval, a four-way control to change the rating, and an
 * optional note in the learner's own words.
 */
@Composable
internal fun ReviewResultContent(
    view: StepView,
    verdict: ReviewVerdict,
    nowPlaying: String?,
    viewModel: StreamViewModel,
) {
    val spacing = MorphoTheme.spacing
    val word = view.word
    Column(verticalArrangement = Arrangement.spacedBy(spacing.sm)) {
        WordPicture(word, Modifier.fillMaxWidth().height(MorphoTheme.sizes.resultPictureHeight))
        StageLabel(StageMark.REVIEW, reviewLabel(view.reps))
        WordTitle(
            word = word,
            playing = nowPlaying == word.word.wordAudioFile,
            onPlay = { viewModel.onPlayCard(word) },
            playLabel = stringResource(R.string.cd_play_card),
        )
        Verdict(task = view.step.task ?: ReviewTask.REBUILD, outcome = verdict.outcome, parts = verdict.parts) {
            DefinitionText(word, compact = true)
        }
        ExampleText(word)
        NoteBlock(view.note)
    }
    Column(
        modifier = Modifier.padding(top = spacing.md),
        verticalArrangement = Arrangement.spacedBy(spacing.sm),
    ) {
        Row(
            verticalAlignment = Alignment.Bottom,
            horizontalArrangement = Arrangement.spacedBy(spacing.xs),
        ) {
            Text(
                text = gradeName(verdict.grade),
                style = MorphoTheme.reading.wordOption,
                color = MaterialTheme.colorScheme.onSurface,
            )
            Text(
                text = stringResource(R.string.stream_next_review, interval(verdict.intervals[verdict.grade] ?: 0)),
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.padding(bottom = spacing.xxs),
            )
        }
        RatingControl(verdict = verdict, onRate = viewModel::onRate)
        Text(
            text = stringResource(R.string.stream_rating_hint),
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        NoteEditor(hasNote = view.note != null, onOpen = viewModel::onTap, onSave = viewModel::onSaveNote)
    }
}

/** How the task went, in the good, close or bad tint, around the definition. */
@Composable
private fun Verdict(task: ReviewTask, outcome: Outcome, parts: ReviewParts?, content: @Composable () -> Unit) {
    val accents = MorphoTheme.accents
    val (line, fill) = when (outcome) {
        Outcome.CLEAN -> accents.correct to accents.correctContainer
        Outcome.SHAKY -> MaterialTheme.colorScheme.tertiary to accents.highlight
        Outcome.FAILED -> accents.wrong to accents.wrongContainer
    }
    val caption = when {
        parts != null -> stringResource(
            R.string.stream_verdict_parts,
            stringResource(rebuildCaption(parts.rebuild)),
            stringResource(spellCaption(parts.spell)),
        )
        task == ReviewTask.REBUILD -> stringResource(rebuildCaption(outcome))
        else -> stringResource(fillCaption(outcome))
    }
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .clip(MorphoTheme.radii.shapeMd)
            .background(fill)
            .border(BorderStroke(1.dp, line), MorphoTheme.radii.shapeMd)
            .padding(horizontal = MorphoTheme.spacing.md, vertical = MorphoTheme.spacing.sm),
        verticalArrangement = Arrangement.spacedBy(MorphoTheme.spacing.xxs),
    ) {
        Text(
            text = caption,
            style = MaterialTheme.typography.labelMedium.copy(fontWeight = FontWeight.SemiBold),
            color = line,
        )
        content()
    }
}

private fun rebuildCaption(outcome: Outcome): Int = when (outcome) {
    Outcome.CLEAN -> R.string.stream_verdict_rebuild_clean
    Outcome.SHAKY -> R.string.stream_verdict_rebuild_shaky
    Outcome.FAILED -> R.string.stream_verdict_rebuild_failed
}

private fun fillCaption(outcome: Outcome): Int = when (outcome) {
    Outcome.CLEAN -> R.string.stream_verdict_fill_clean
    Outcome.SHAKY -> R.string.stream_verdict_fill_shaky
    Outcome.FAILED -> R.string.stream_verdict_fill_failed
}

private fun spellCaption(outcome: Outcome): Int = when (outcome) {
    Outcome.CLEAN -> R.string.stream_verdict_spell_clean
    Outcome.SHAKY -> R.string.stream_verdict_spell_shaky
    Outcome.FAILED -> R.string.stream_verdict_spell_failed
}

/** Again / Hard / Good / Easy, each with the interval it would give; the chosen one tinted. */
@Composable
private fun RatingControl(verdict: ReviewVerdict, onRate: (Grade) -> Unit) {
    val accents = MorphoTheme.accents
    val colors = MaterialTheme.colorScheme
    val shape = MorphoTheme.radii.shapeSm
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .height(IntrinsicSize.Min)
            .clip(shape)
            .border(BorderStroke(1.dp, colors.outlineVariant), shape)
            .selectableGroup(),
    ) {
        Grade.entries.forEachIndexed { k, grade ->
            val selected = verdict.grade == grade
            val (fill, ink) = when {
                !selected -> colors.surfaceContainerLow to colors.onSurfaceVariant
                grade == Grade.AGAIN -> accents.wrongContainer to accents.wrong
                grade == Grade.HARD -> accents.highlight to colors.tertiary
                else -> accents.correctContainer to accents.correct
            }
            if (k > 0) VerticalDivider(color = colors.outlineVariant)
            Column(
                modifier = Modifier
                    .weight(1f)
                    .heightIn(min = MorphoTheme.spacing.minTouchTarget)
                    .pressMotion()
                    .background(fill)
                    .selectable(selected = selected, role = Role.RadioButton, onClick = { onRate(grade) })
                    .padding(vertical = MorphoTheme.spacing.xs),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.Center,
            ) {
                Text(
                    text = gradeName(grade),
                    style = MaterialTheme.typography.labelMedium.copy(
                        fontWeight = if (selected) FontWeight.SemiBold else FontWeight.Normal,
                    ),
                    color = ink,
                )
                Text(
                    text = interval(verdict.intervals[grade] ?: 0),
                    style = MaterialTheme.typography.labelSmall,
                    color = ink.copy(alpha = 0.8f),
                )
            }
        }
    }
}

/** "Say it in your own words": a link that opens a field and a Save button. */
@Composable
private fun NoteEditor(hasNote: Boolean, onOpen: () -> Unit, onSave: (String) -> Unit) {
    var writing by rememberSaveable { mutableStateOf(false) }
    var text by rememberSaveable { mutableStateOf("") }
    if (!writing) {
        TextButton(
            onClick = {
                onOpen()
                writing = true
            },
            modifier = Modifier
                .fillMaxWidth()
                .pressMotion(),
        ) {
            Text(
                text = stringResource(if (hasNote) R.string.stream_note_rewrite else R.string.stream_note_add),
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        return
    }
    val save = {
        if (text.isNotBlank()) {
            onSave(text)
            writing = false
            text = ""
        }
    }
    Column(verticalArrangement = Arrangement.spacedBy(MorphoTheme.spacing.xs)) {
        OutlinedTextField(
            value = text,
            onValueChange = { text = it },
            label = { Text(stringResource(R.string.stream_note_title)) },
            placeholder = { Text(stringResource(R.string.stream_note_placeholder)) },
            minLines = 2,
            keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences, imeAction = ImeAction.Done),
            keyboardActions = KeyboardActions(onDone = { save() }),
            modifier = Modifier.fillMaxWidth(),
        )
        TextButton(
            onClick = save,
            modifier = Modifier
                .align(Alignment.End)
                .pressMotion(text.isNotBlank()),
            enabled = text.isNotBlank(),
        ) {
            Text(stringResource(R.string.action_save))
        }
    }
}

@Composable
private fun gradeName(grade: Grade): String = stringResource(
    when (grade) {
        Grade.AGAIN -> R.string.grade_again
        Grade.HARD -> R.string.grade_hard
        Grade.GOOD -> R.string.grade_good
        Grade.EASY -> R.string.grade_easy
    },
)

/** A review interval in days, as days up to a month, then months, then years. */
@Composable
internal fun interval(days: Long): String {
    val d = days.coerceAtLeast(1).toInt()
    if (d < DAYS_PER_MONTH_SHOWN) return pluralStringResource(R.plurals.interval_days, d, d)
    val months = Math.round(d / DAYS_PER_MONTH).toInt()
    if (months < 12) return pluralStringResource(R.plurals.interval_months, months, months)
    val years = d / DAYS_PER_YEAR
    return stringResource(R.string.interval_years, String.format(java.util.Locale.US, "%.1f", years).removeSuffix(".0"))
}

private const val DAYS_PER_MONTH_SHOWN = 31
private const val DAYS_PER_MONTH = 30.4
private const val DAYS_PER_YEAR = 365.0
