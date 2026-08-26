package dev.morpho.ui

import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Surface
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.unit.dp
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import dev.morpho.ui.designsystem.component.RetryHelpCard
import dev.morpho.ui.designsystem.component.SenseDetail
import dev.morpho.ui.designsystem.theme.MorphoTheme
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File
import java.io.FileOutputStream

/**
 * THROWAWAY: renders the real wrong-answer card in both themes and writes PNGs to the
 * app external files dir so they can be pulled for review. Not part of the standing suite.
 */
@RunWith(AndroidJUnit4::class)
class RetryHelpCardCaptureTest {

    @get:Rule
    val composeRule = createComposeRule()

    private val senses = listOf(
        SenseDetail(
            "adj",
            "kind and generous towards other people, especially those with less power",
            true,
            "audio/def1.ogg",
        ),
        SenseDetail(
            "adj",
            "wishing to do good and to help, rather than to gain something",
            false,
            "audio/def2.ogg",
        ),
    )

    @Test
    fun captureLight() = capture(darkTheme = false, name = "retry_help_card_light")

    @Test
    fun captureDark() = capture(darkTheme = true, name = "retry_help_card_dark")

    private fun capture(darkTheme: Boolean, name: String) {
        composeRule.setContent {
            MorphoTheme(darkTheme = darkTheme) {
                Surface {
                    RetryHelpCard(
                        hint = "Not this one. Read the meaning, then pick again.",
                        word = "benevolent",
                        phonetic = "/bəˈnevələnt/",
                        senses = senses,
                        onPlayWord = {},
                        modifier = Modifier
                            .testTag("card")
                            .padding(16.dp),
                    )
                }
            }
        }
        val bitmap = composeRule.onNodeWithTag("card").captureToImage().asAndroidBitmap()
        val dir = InstrumentationRegistry.getInstrumentation().targetContext.filesDir
        val file = File(dir, "$name.png")
        FileOutputStream(file).use { out ->
            bitmap.compress(android.graphics.Bitmap.CompressFormat.PNG, 100, out)
        }
    }
}
