package dev.morpho.ui

import androidx.compose.foundation.layout.Column
import androidx.compose.material3.Surface
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.performClick
import androidx.test.ext.junit.runners.AndroidJUnit4
import dev.morpho.ui.designsystem.component.GroupProgressBar
import dev.morpho.ui.designsystem.component.GroupSegmentState
import dev.morpho.ui.designsystem.component.ImageOption
import dev.morpho.ui.designsystem.component.ModePips
import dev.morpho.ui.designsystem.component.ProgressRing
import dev.morpho.ui.designsystem.component.QuizImageGrid
import dev.morpho.ui.designsystem.component.QuizTextOptions
import dev.morpho.ui.designsystem.component.SentenceCard
import dev.morpho.ui.designsystem.component.TextOption
import dev.morpho.ui.designsystem.component.WordHeader
import dev.morpho.ui.designsystem.theme.MorphoTheme
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * Render smoke tests for the design system, in both themes.
 *
 * Wave-1 bar per docs/contracts/conventions.md: components must compose, lay out and
 * expose their accessibility labels. Requires a connected device or emulator.
 */
@RunWith(AndroidJUnit4::class)
class DesignSystemSmokeTest {

    @get:Rule
    val composeRule = createComposeRule()

    private val options = List(4) { i ->
        ImageOption(
            wordId = i.toLong(),
            imageFile = "img/preview-$i.webp",
            caption = "definition $i",
            accessibilityLabel = "Option ${i + 1} of 4",
        )
    }

    @Test
    fun componentsRenderInLightTheme() = renderAll(darkTheme = false)

    @Test
    fun componentsRenderInDarkTheme() = renderAll(darkTheme = true)

    private fun renderAll(darkTheme: Boolean) {
        composeRule.setContent {
            MorphoTheme(darkTheme = darkTheme) {
                Surface {
                    Column {
                        WordHeader(
                            word = "benevolent",
                            phonetic = "/bəˈnevələnt/",
                            onPlayAudio = {},
                        )
                        ModePips(mode = 2)
                        GroupProgressBar(segments = List(8) { GroupSegmentState.PENDING })
                        ProgressRing(
                            progress = 0.4f,
                            centerLabel = "2,090",
                            centerCaption = "of 5,500 learned",
                        )
                    }
                }
            }
        }
        composeRule.onNodeWithText("benevolent").assertIsDisplayed()
        composeRule.onNodeWithContentDescription("Learning mode 2 of 3").assertIsDisplayed()
    }

    @Test
    fun quizImageGridReportsSelection() {
        var selected = -1
        composeRule.setContent {
            MorphoTheme {
                Surface {
                    QuizImageGrid(options = options, onSelect = { selected = it })
                }
            }
        }
        composeRule.onNodeWithContentDescription("Option 3 of 4").performClick()
        assert(selected == 2) { "expected option index 2, got $selected" }
    }

    @Test
    fun quizTextOptionsRenderEveryDefinition() {
        composeRule.setContent {
            MorphoTheme {
                Surface {
                    QuizTextOptions(
                        options = List(4) { TextOption(it.toLong(), "meaning number $it", "adj") },
                        onSelect = {},
                    )
                }
            }
        }
        repeat(4) { i ->
            composeRule.onNodeWithText("meaning number $i").assertIsDisplayed()
        }
    }

    @Test
    fun sentenceCardHighlightsAndPlays() {
        var played = false
        composeRule.setContent {
            MorphoTheme {
                Surface {
                    SentenceCard(
                        sentence = "A benevolent stranger paid for the table.",
                        highlight = 2..11,
                        onPlayAudio = { played = true },
                    )
                }
            }
        }
        composeRule
            .onNodeWithContentDescription(
                "Example sentence. A benevolent stranger paid for the table.. Tap to listen.",
            )
            .performClick()
        assert(played) { "tapping the sentence card should play its audio" }
    }
}
