package dev.morpho.ui.designsystem.component

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.Dp
import dev.morpho.ui.designsystem.theme.MorphoTheme

/**
 * Share of the viewport height the answer zone may claim before the prompt is asked for
 * anything. 0.62 keeps a 2x2 image grid comfortably inside the lower half on every phone
 * we ship to, and still leaves a readable band for the sentence card above it.
 */
private const val ANSWER_MAX_HEIGHT_FRACTION = 0.62f

/** Landscape: answers own the right-hand pane, the prompt reads down the left. */
private const val LANDSCAPE_ANSWER_WIDTH_FRACTION = 0.56f

/**
 * The width the answer zone spans and the tallest it is allowed to become.
 *
 * Handed to the `answers` slot by [QuizLayout] so a grid sizes its cells against the
 * space it was actually granted instead of guessing at the screen and overflowing.
 * [Dp.Infinity] as [maxHeight] means "no cap" — the fallback path for unbounded hosts.
 */
@Immutable
data class QuizAnswerSpace(
    val maxWidth: Dp,
    val maxHeight: Dp,
)

/**
 * The skeleton every quiz screen wears: **the answers own the bottom of the viewport.**
 *
 * Owner ruling, wave 3c — the option grid belongs in the thumb zone, not floating in the
 * middle of the screen. So the body is three bands instead of one scrolling column:
 *
 * ```
 * header     fixed, hugging the top bar (group progress)
 * prompt     flexible; swallows every spare pixel and scrolls inside itself
 * banner     retry hint / feedback, pinned directly above the answers
 * answers    pinned to the bottom, height capped at ANSWER_MAX_HEIGHT_FRACTION
 * ```
 *
 * Two properties fall out of that, and they are the entire point:
 *
 * * **The answer zone never moves.** Its height is derived from the viewport, never from
 *   its siblings, so a banner appearing above it compresses the (scrollable) prompt
 *   rather than shoving the grid out from under the user's thumb. That is why the retry
 *   hint lives in its own slot between prompt and answers instead of below the grid.
 * * **The answer zone never gets squeezed.** It claims its share first; on a short screen
 *   an image grid shrinks its cells to fit that envelope and the prompt scrolls, instead
 *   of the grid spilling past the bottom edge.
 *
 * Bottom padding above the navigation bar comes from the caller's `Scaffold` inner
 * padding (the activity is edge-to-edge) plus one spacing step here, so the last row of
 * options clears the gesture bar.
 *
 * Landscape splits left/right instead: a 2x2 grid under a ~360dp-tall viewport would
 * leave nothing for the sentence, so the prompt reads down one side and the answers fill
 * the other — still the side the thumb reaches. An unbounded height (`@Preview`,
 * wrap-content hosts) degrades to one plain scrolling column.
 */
@Composable
fun QuizLayout(
    modifier: Modifier = Modifier,
    header: @Composable () -> Unit = {},
    banner: @Composable () -> Unit = {},
    prompt: @Composable ColumnScope.() -> Unit,
    answers: @Composable (QuizAnswerSpace) -> Unit,
) {
    val spacing = MorphoTheme.spacing
    BoxWithConstraints(
        modifier = modifier
            .fillMaxSize()
            // The answer zone is the bottom of the viewport, and on the listening-spell
            // question the bottom of the viewport is where the keyboard opens. Under
            // edge-to-edge the window does not resize itself, so the layout has to take
            // the IME inset or it would anchor its own input out of sight.
            .imePadding()
            .padding(horizontal = spacing.screenGutter)
            .padding(top = spacing.xs, bottom = spacing.md),
    ) {
        when {
            !constraints.hasBoundedHeight -> StackedFallback(
                space = QuizAnswerSpace(maxWidth, Dp.Infinity),
                header = header,
                banner = banner,
                prompt = prompt,
                answers = answers,
            )

            maxWidth > maxHeight -> LandscapeSplit(
                space = QuizAnswerSpace(
                    maxWidth = (maxWidth - spacing.lg) * LANDSCAPE_ANSWER_WIDTH_FRACTION,
                    maxHeight = maxHeight,
                ),
                header = header,
                banner = banner,
                prompt = prompt,
                answers = answers,
            )

            else -> BottomAnchored(
                space = QuizAnswerSpace(
                    maxWidth = maxWidth,
                    maxHeight = maxHeight * ANSWER_MAX_HEIGHT_FRACTION,
                ),
                header = header,
                banner = banner,
                prompt = prompt,
                answers = answers,
            )
        }
    }
}

@Composable
private fun BottomAnchored(
    space: QuizAnswerSpace,
    header: @Composable () -> Unit,
    banner: @Composable () -> Unit,
    prompt: @Composable ColumnScope.() -> Unit,
    answers: @Composable (QuizAnswerSpace) -> Unit,
) {
    val spacing = MorphoTheme.spacing
    Column(
        modifier = Modifier.fillMaxSize(),
        verticalArrangement = Arrangement.spacedBy(spacing.md),
    ) {
        header()
        // The one weighted band: whatever the header and the answers leave behind lands
        // here, which is what pins the answers to the bottom on a tall screen.
        Column(
            modifier = Modifier
                .weight(1f)
                .fillMaxWidth(),
            verticalArrangement = Arrangement.spacedBy(spacing.sm),
        ) {
            Column(
                modifier = Modifier
                    .weight(1f)
                    .fillMaxWidth()
                    .verticalScroll(rememberScrollState()),
                verticalArrangement = Arrangement.spacedBy(spacing.md),
                content = prompt,
            )
            banner()
        }
        AnswerZone(space = space, alignment = Alignment.BottomCenter, answers = answers)
    }
}

@Composable
private fun LandscapeSplit(
    space: QuizAnswerSpace,
    header: @Composable () -> Unit,
    banner: @Composable () -> Unit,
    prompt: @Composable ColumnScope.() -> Unit,
    answers: @Composable (QuizAnswerSpace) -> Unit,
) {
    val spacing = MorphoTheme.spacing
    Row(
        modifier = Modifier.fillMaxSize(),
        horizontalArrangement = Arrangement.spacedBy(spacing.lg),
    ) {
        Column(
            modifier = Modifier
                .weight(1f - LANDSCAPE_ANSWER_WIDTH_FRACTION)
                .fillMaxHeight(),
            verticalArrangement = Arrangement.spacedBy(spacing.sm),
        ) {
            header()
            Column(
                modifier = Modifier
                    .weight(1f)
                    .fillMaxWidth()
                    .verticalScroll(rememberScrollState()),
                verticalArrangement = Arrangement.spacedBy(spacing.md),
                content = prompt,
            )
            banner()
        }
        AnswerZone(
            space = space,
            alignment = Alignment.Center,
            answers = answers,
            modifier = Modifier
                .weight(LANDSCAPE_ANSWER_WIDTH_FRACTION)
                .fillMaxHeight(),
        )
    }
}

/**
 * No bounded height to anchor against — a `@Preview` canvas or any wrap-content host.
 * Falls back to the pre-wave-3c reading: one column, everything scrolls together.
 */
@Composable
private fun StackedFallback(
    space: QuizAnswerSpace,
    header: @Composable () -> Unit,
    banner: @Composable () -> Unit,
    prompt: @Composable ColumnScope.() -> Unit,
    answers: @Composable (QuizAnswerSpace) -> Unit,
) {
    val spacing = MorphoTheme.spacing
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .verticalScroll(rememberScrollState()),
        verticalArrangement = Arrangement.spacedBy(spacing.md),
    ) {
        header()
        prompt()
        banner()
        answers(space)
    }
}

/**
 * The zone is capped, and scrolls rather than spills. An image grid sizes itself to
 * [space] and never needs the scroll; text cards and a 2x accessibility font scale can
 * both outgrow the envelope, and when they do the overflow must stay inside the zone
 * instead of painting over the navigation bar.
 */
@Composable
private fun AnswerZone(
    space: QuizAnswerSpace,
    alignment: Alignment,
    answers: @Composable (QuizAnswerSpace) -> Unit,
    modifier: Modifier = Modifier,
) {
    Box(
        modifier = modifier
            .fillMaxWidth()
            .heightIn(max = space.maxHeight)
            .verticalScroll(rememberScrollState()),
        contentAlignment = alignment,
    ) {
        answers(space)
    }
}
