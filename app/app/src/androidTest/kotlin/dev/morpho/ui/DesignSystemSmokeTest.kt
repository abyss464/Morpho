package dev.morpho.ui

import androidx.compose.foundation.layout.Column
import androidx.compose.material3.Surface
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.test.TouchInjectionScope
import androidx.compose.ui.test.click
import androidx.compose.ui.test.longClick
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTouchInput
import androidx.test.ext.junit.runners.AndroidJUnit4
import dev.morpho.domain.content.GlossAnchor
import dev.morpho.domain.content.GlossIndex
import dev.morpho.ui.designsystem.component.GroupProgressBar
import dev.morpho.ui.designsystem.component.GroupSegmentState
import dev.morpho.ui.designsystem.component.ImageOption
import dev.morpho.ui.designsystem.component.LocalGlossIndex
import dev.morpho.ui.designsystem.component.ModePips
import dev.morpho.ui.designsystem.component.ProgressRing
import dev.morpho.ui.designsystem.component.QuizImageGrid
import dev.morpho.ui.designsystem.component.QuizTextOptions
import dev.morpho.ui.designsystem.component.RetryHelpCard
import dev.morpho.ui.designsystem.component.SenseDetail
import dev.morpho.ui.designsystem.component.SentenceCard
import dev.morpho.ui.designsystem.component.TextOption
import androidx.compose.ui.unit.dp
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

    private val glossIndex = GlossIndex.of(listOf(GlossAnchor(1, "intricate", "错综复杂的")))

    /** The anchored word leads, so a fixed fraction of the width lands on it. */
    private val GLOSSED_OPTION = "intricate lattices of frost"

    private fun TouchInjectionScope.onTheAnchor() = Offset(width * 0.12f, height / 2f)

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

    /**
     * The one thing the gloss mechanism must never do: turn a tap on an option's words
     * into "I don't know what that means" instead of "I choose this". The gloss lives on
     * a long press precisely so this stays true.
     *
     * Both of these drive the real pointer path — an unmerged node and injected touches —
     * rather than invoking the card's semantic `onClick`, which would sail straight past
     * the gesture detector the tests exist to check.
     */
    @Test
    fun tappingAGlossedWordInsideAnOptionStillSelectsTheOption() {
        var selected = -1
        composeRule.setContent {
            CompositionLocalProvider(LocalGlossIndex provides glossIndex) {
                MorphoTheme {
                    Surface {
                        QuizTextOptions(
                            options = listOf(
                                TextOption(1, GLOSSED_OPTION, "adj"),
                                TextOption(2, "plain woven cloth", "adj"),
                            ),
                            onSelect = { selected = it },
                        )
                    }
                }
            }
        }
        composeRule.onNodeWithText(GLOSSED_OPTION, useUnmergedTree = true)
            .performTouchInput { click(onTheAnchor()) }
        assert(selected == 0) { "expected option 0 to be selected, got $selected" }
    }

    @Test
    fun aGlossedWordRevealsItsChineseOnLongPress() {
        composeRule.setContent {
            CompositionLocalProvider(LocalGlossIndex provides glossIndex) {
                MorphoTheme {
                    Surface {
                        QuizTextOptions(
                            options = listOf(TextOption(1, GLOSSED_OPTION, "adj")),
                            onSelect = {},
                        )
                    }
                }
            }
        }
        composeRule.onNodeWithText(GLOSSED_OPTION, useUnmergedTree = true)
            .performTouchInput { longClick(onTheAnchor()) }
        // The popover is its own window, so assert on existence rather than display.
        composeRule.onNodeWithText("错综复杂的").assertExists()
    }

    /**
     * The wrong-answer card must teach the *whole* word on a miss: the word itself, its
     * pronunciation button, and every selected sense — not just the primary gloss. A
     * generous height keeps all senses on screen for the assertion instead of scrolling
     * the last one out of view.
     */
    @Test
    fun retryHelpCardShowsWordAndEverySense() {
        val senses = listOf(
            SenseDetail("adj", "kind and generous towards other people", true, "audio/def1.ogg"),
            SenseDetail("adj", "wishing to do good rather than to gain", false, "audio/def2.ogg"),
            SenseDetail("noun", "a benevolent person or influence", false, "audio/def3.ogg"),
        )
        var played = false
        composeRule.setContent {
            MorphoTheme {
                Surface {
                    RetryHelpCard(
                        hint = "Not this one. Read the meaning, then pick again.",
                        word = "benevolent",
                        phonetic = "/bəˈnevələnt/",
                        senses = senses,
                        onPlayWord = { played = true },
                        maxHeight = 2000.dp,
                    )
                }
            }
        }
        composeRule.onNodeWithText("benevolent").assertIsDisplayed()
        senses.forEach { sense ->
            composeRule.onNodeWithText(sense.definition).assertIsDisplayed()
        }
        composeRule
            .onNodeWithContentDescription("Play pronunciation of benevolent")
            .performClick()
        assert(played) { "the card's word audio button should play the word" }
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
