package dev.morpho.ui.summary

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.AutoAwesome
import androidx.compose.material.icons.rounded.Refresh
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import dev.morpho.R
import dev.morpho.data.sound.SfxEvent
import dev.morpho.di.AppContainer
import dev.morpho.di.SessionKind
import dev.morpho.di.SessionResult
import dev.morpho.ui.designsystem.component.PreviewBox
import dev.morpho.ui.designsystem.component.SessionSummaryCard
import dev.morpho.ui.designsystem.component.StatTile
import dev.morpho.ui.designsystem.component.StreakBadge
import dev.morpho.ui.designsystem.component.ThemePreviews
import dev.morpho.ui.designsystem.theme.MorphoTheme

/** End-of-session recap: what was learned, what was reviewed, how accurate. */
@Composable
fun SessionSummaryScreen(
    container: AppContainer,
    kind: SessionKind,
    onBackHome: () -> Unit,
    onKeepGoing: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val result = container.sessionResults.consume()
        ?: SessionResult(kind, 0, 0, 0, 0, 0, false)

    Scaffold(modifier = modifier.fillMaxSize()) { padding ->
        SummaryContent(
            result = result,
            modifier = Modifier
                .fillMaxSize()
                .padding(padding),
            onBackHome = {
                container.playSfx(SfxEvent.TAP)
                onBackHome()
            },
            onKeepGoing = {
                container.playSfx(SfxEvent.TAP)
                onKeepGoing()
            },
        )
    }
}

@Composable
private fun SummaryContent(
    result: SessionResult,
    onBackHome: () -> Unit,
    onKeepGoing: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val spacing = MorphoTheme.spacing
    Column(
        modifier = modifier
            .padding(horizontal = spacing.screenGutter)
            .padding(vertical = spacing.xl),
        verticalArrangement = Arrangement.spacedBy(spacing.lg),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Text(
            text = stringResource(R.string.summary_title),
            style = MaterialTheme.typography.headlineMedium,
            color = MaterialTheme.colorScheme.onSurface,
        )

        SessionSummaryCard(
            headline = stringResource(
                if (result.kind == SessionKind.REVIEW) {
                    R.string.summary_review_headline
                } else {
                    R.string.summary_learning_headline
                },
            ),
            supporting = supportingText(result),
        ) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(spacing.sm),
            ) {
                if (result.kind == SessionKind.LEARNING) {
                    StatTile(
                        value = result.newLearned.toString(),
                        label = stringResource(R.string.summary_new_words),
                        icon = Icons.Rounded.AutoAwesome,
                        emphasis = true,
                        modifier = Modifier.weight(1f),
                    )
                } else {
                    StatTile(
                        value = result.reviewed.toString(),
                        label = stringResource(R.string.summary_reviewed),
                        icon = Icons.Rounded.Refresh,
                        emphasis = true,
                        modifier = Modifier.weight(1f),
                    )
                }
                StatTile(
                    value = result.accuracy?.let { "${(it * 100).toInt()}%" } ?: "—",
                    label = stringResource(R.string.summary_accuracy),
                    modifier = Modifier.weight(1f),
                )
            }
        }

        if (result.streakDays > 0) {
            StreakBadge(days = result.streakDays)
        }

        Spacer(Modifier.height(spacing.md))

        Button(
            onClick = onBackHome,
            modifier = Modifier
                .fillMaxWidth()
                .heightIn(min = spacing.minTouchTarget),
            shape = MorphoTheme.radii.shapeLg,
        ) {
            Text(stringResource(R.string.summary_back_home))
        }

        if (result.kind == SessionKind.LEARNING && !result.goalMet) {
            OutlinedButton(
                onClick = onKeepGoing,
                modifier = Modifier
                    .fillMaxWidth()
                    .heightIn(min = spacing.minTouchTarget),
                shape = MorphoTheme.radii.shapeLg,
            ) {
                Text(stringResource(R.string.summary_keep_going))
            }
        }
    }
}

@Composable
private fun supportingText(result: SessionResult): String {
    val accuracy = result.accuracy?.let { " at ${(it * 100).toInt()}% first-try accuracy" } ?: ""
    return when {
        result.kind == SessionKind.REVIEW ->
            "${result.reviewed} words reviewed$accuracy."

        result.newLearned == 0 ->
            "No words finished all three rounds this time — they will come back next session."

        else -> "${result.newLearned} words cleared all three rounds$accuracy."
    }
}

@ThemePreviews
@Composable
private fun SummaryPreview() {
    PreviewBox {
        SummaryContent(
            result = SessionResult(
                kind = SessionKind.LEARNING,
                newLearned = 18,
                reviewed = 0,
                correctFirstTry = 51,
                totalFirstTry = 54,
                streakDays = 12,
                goalMet = false,
            ),
            onBackHome = {},
            onKeepGoing = {},
        )
    }
}

@ThemePreviews
@Composable
private fun SummaryReviewPreview() {
    PreviewBox {
        SummaryContent(
            result = SessionResult(
                kind = SessionKind.REVIEW,
                newLearned = 0,
                reviewed = 26,
                correctFirstTry = 23,
                totalFirstTry = 26,
                streakDays = 3,
                goalMet = false,
            ),
            onBackHome = {},
            onKeepGoing = {},
        )
    }
}
