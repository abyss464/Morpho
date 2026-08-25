package dev.morpho.ui.designsystem.component

import android.content.res.Configuration
import androidx.compose.foundation.layout.Box
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
