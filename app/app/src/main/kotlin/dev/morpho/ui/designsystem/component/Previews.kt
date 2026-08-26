package dev.morpho.ui.designsystem.component

import android.content.res.Configuration
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Surface
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.tooling.preview.Preview
import dev.morpho.ui.designsystem.theme.MorphoTheme

/**
 * Every design-system component previews in both themes, as the contract requires.
 * Annotate a preview function with [ThemePreviews] and wrap its body in [PreviewBox].
 */
@Preview(name = "Light", showBackground = true, widthDp = 400)
@Preview(
    name = "Dark",
    showBackground = true,
    widthDp = 400,
    uiMode = Configuration.UI_MODE_NIGHT_YES,
)
annotation class ThemePreviews

@Composable
fun PreviewBox(
    darkTheme: Boolean = androidx.compose.foundation.isSystemInDarkTheme(),
    content: @Composable () -> Unit,
) {
    MorphoTheme(darkTheme = darkTheme) {
        Surface {
            Box(Modifier.padding(MorphoTheme.spacing.md)) { content() }
        }
    }
}

/**
 * Whole-screen previews. A bottom-anchored layout only tells the truth against a
 * **bounded** height, so these pin one, and cover the two shapes that break naive
 * anchoring: a short phone and a sideways one.
 */
@Preview(name = "Light", showBackground = true, widthDp = 400, heightDp = 840)
@Preview(
    name = "Dark",
    showBackground = true,
    widthDp = 400,
    heightDp = 840,
    uiMode = Configuration.UI_MODE_NIGHT_YES,
)
@Preview(name = "Short", showBackground = true, widthDp = 360, heightDp = 592)
@Preview(name = "Landscape", showBackground = true, widthDp = 780, heightDp = 380)
annotation class ScreenPreviews

/** Host for [ScreenPreviews]: no gutter of its own — screens bring their own. */
@Composable
fun ScreenPreviewBox(
    darkTheme: Boolean = androidx.compose.foundation.isSystemInDarkTheme(),
    content: @Composable () -> Unit,
) {
    MorphoTheme(darkTheme = darkTheme) {
        Surface(Modifier.fillMaxSize()) { content() }
    }
}
