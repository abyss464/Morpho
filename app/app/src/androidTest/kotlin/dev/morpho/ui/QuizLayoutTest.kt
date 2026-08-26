package dev.morpho.ui

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.requiredSize
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.getUnclippedBoundsInRoot
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.DpRect
import androidx.compose.ui.unit.dp
import androidx.test.ext.junit.runners.AndroidJUnit4
import dev.morpho.ui.designsystem.component.ImageOption
import dev.morpho.ui.designsystem.component.QuizAnswerSpace
import dev.morpho.ui.designsystem.component.QuizImageDefGrid
import dev.morpho.ui.designsystem.component.QuizImageGrid
import dev.morpho.ui.designsystem.component.QuizLayout
import dev.morpho.ui.designsystem.component.QuizTextOptions
import dev.morpho.ui.designsystem.component.TextOption
import dev.morpho.ui.designsystem.theme.MorphoTheme
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * Wave-3c owner ruling: the options live in the thumb zone.
 *
 * These are layout assertions, not render smoke — every one of them fails if the answer
 * zone drifts back up the screen, moves when a banner appears, or spills past the bottom
 * edge on a short viewport. Each test pins its own viewport with a fixed-size `Box`, so
 * the numbers hold whatever device or emulator runs them.
 */
@RunWith(AndroidJUnit4::class)
class QuizLayoutTest {

    @get:Rule
    val composeRule = createComposeRule()

    private val options = List(4) { i ->
        ImageOption(
            wordId = i.toLong(),
            imageFile = "img/preview-$i.webp",
            caption = "a fairly long one line definition for cell ${i + 1}",
            accessibilityLabel = "Option ${i + 1} of 4",
        )
    }

    private fun cell(index: Int): DpRect =
        composeRule.onNodeWithContentDescription("Option $index of 4").getUnclippedBoundsInRoot()

    /** Everything is laid out to the dp; a hair of rounding slack is still fair. */
    private fun assertNear(expected: Dp, actual: Dp, message: String) {
        assert(kotlin.math.abs(expected.value - actual.value) <= 1f) {
            "$message: expected ~$expected, got $actual"
        }
    }

    @Composable
    private fun Viewport(
        width: Dp,
        height: Dp,
        banner: @Composable () -> Unit = {},
        answers: @Composable (QuizAnswerSpace) -> Unit,
    ) {
        MorphoTheme {
            Surface {
                Box(Modifier.requiredSize(width, height)) {
                    QuizLayout(
                        header = { Text(HEADER) },
                        banner = banner,
                        prompt = { Text(PROMPT) },
                        answers = answers,
                    )
                }
            }
        }
    }

    @Test
    fun imageGridIsAnchoredToTheLowerHalfOfTheViewport() {
        composeRule.setContent {
            Viewport(width = PHONE_W, height = PHONE_H) { space ->
                QuizImageGrid(options = options, onSelect = {}, space = space)
            }
        }

        val first = cell(1)
        val last = cell(4)
        assert(first.top >= PHONE_H / 2f) {
            "grid should start below the halfway line, started at ${first.top} of $PHONE_H"
        }
        // Bottom-anchored: exactly one spacing step of breathing room, nothing more.
        assertNear(PHONE_H - BOTTOM_GUTTER, last.bottom, "grid bottom")
    }

    @Test
    fun captionedGridStillEndsAtTheBottomGutter() {
        composeRule.setContent {
            Viewport(width = PHONE_W, height = PHONE_H) { space ->
                QuizImageDefGrid(options = options, onSelect = {}, space = space)
            }
        }
        assertNear(PHONE_H - BOTTOM_GUTTER, cell(4).bottom, "captioned grid bottom")
        // Captions are a fixed band, so the two cells of a row end level with each other.
        assertNear(cell(3).bottom, cell(4).bottom, "row cells")
    }

    @Test
    fun textOptionsAreAnchoredToo() {
        composeRule.setContent {
            Viewport(width = PHONE_W, height = PHONE_H) {
                QuizTextOptions(
                    options = List(4) { TextOption(it.toLong(), "meaning number $it", "adj") },
                    onSelect = {},
                )
            }
        }
        val last = composeRule.onNodeWithContentDescription("meaning number 3")
            .getUnclippedBoundsInRoot()
        assertNear(PHONE_H - BOTTOM_GUTTER, last.bottom, "last text option bottom")
    }

    /**
     * The reason the retry hint moved out from under the grid: a banner that appears
     * after a wrong answer must not shift the target the thumb is already aiming at.
     */
    @Test
    fun theBannerDoesNotMoveTheGrid() {
        var showBanner by mutableStateOf(false)
        composeRule.setContent {
            Viewport(
                width = PHONE_W,
                height = PHONE_H,
                banner = { if (showBanner) Text(BANNER) },
            ) { space ->
                QuizImageGrid(options = options, onSelect = {}, space = space)
            }
        }

        val before = cell(1)
        composeRule.runOnIdle { showBanner = true }
        composeRule.onNodeWithText(BANNER).assertIsDisplayed()
        val after = cell(1)

        assert(before == after) { "grid moved when the banner appeared: $before -> $after" }
    }

    /**
     * The short viewport is the case that used to squeeze. The cells shrink to fit the
     * envelope instead of the zone giving way: smaller than their natural 4:3, never
     * below the readability floor, still ending on the bottom gutter.
     */
    @Test
    fun aShortViewportShrinksTheCellsRatherThanTheZone() {
        composeRule.setContent {
            Viewport(width = SHORT_W, height = SHORT_H) { space ->
                QuizImageGrid(options = options, onSelect = {}, space = space)
            }
        }

        val first = cell(1)
        val cellHeight = first.bottom - first.top
        assert(cellHeight < naturalCellHeight(SHORT_W)) {
            "cell should have shrunk below its natural 4:3, measured $cellHeight"
        }
        assert(cellHeight >= MIN_IMAGE_BAND) {
            "cell collapsed below the readability floor: $cellHeight"
        }
        assertNear(SHORT_H - BOTTOM_GUTTER, cell(4).bottom, "short-viewport grid bottom")
        composeRule.onNodeWithText(PROMPT).assertIsDisplayed()
    }

    /** What a cell would be at its own 4:3, given the screen gutter and the grid gutter. */
    private fun naturalCellHeight(viewportWidth: Dp): Dp =
        (viewportWidth - SCREEN_GUTTER * 2 - GRID_GUTTER) / 2f / (4f / 3f)

    /**
     * Sideways there is no lower half worth having, so the answers take a side instead.
     *
     * Asserted as a *relationship* rather than against the requested width on purpose:
     * a landscape viewport wider than the host window gets clamped to it, and the
     * property that matters — read on one side, answer on the other, two rows still
     * inside the viewport — survives the clamp.
     */
    @Test
    fun landscapePutsTheAnswersBesideThePromptRatherThanUnderIt() {
        composeRule.setContent {
            Viewport(width = LANDSCAPE_W, height = LANDSCAPE_H) { space ->
                QuizImageGrid(options = options, onSelect = {}, space = space)
            }
        }

        val first = cell(1)
        val prompt = composeRule.onNodeWithText(PROMPT).getUnclippedBoundsInRoot()
        assert(prompt.right <= first.left) {
            "prompt (ends ${prompt.right}) should sit left of the answers (start ${first.left})"
        }
        assert(prompt.top < first.bottom) { "the two panes should share the vertical band" }
        assert(cell(4).bottom <= LANDSCAPE_H) {
            "both rows should fit the short viewport, ended at ${cell(4).bottom}"
        }
    }

    private companion object {
        val PHONE_W = 360.dp
        val PHONE_H = 720.dp
        val SHORT_W = 360.dp
        val SHORT_H = 400.dp
        val LANDSCAPE_W = 720.dp
        val LANDSCAPE_H = 340.dp

        /** `QuizLayout`'s own bottom padding, on top of whatever insets the caller passes. */
        val BOTTOM_GUTTER = 16.dp

        /** `Sizes.quizImageMinBand`. */
        val MIN_IMAGE_BAND = 96.dp

        /** `Spacing.screenGutter` and the `Spacing.sm` gutter between cells. */
        val SCREEN_GUTTER = 16.dp
        val GRID_GUTTER = 12.dp

        const val HEADER = "header"
        const val PROMPT = "the prompt the user reads"
        const val BANNER = "try again"
    }
}
