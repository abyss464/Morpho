package dev.morpho.ui.designsystem.component

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import dev.morpho.ui.designsystem.motion.pressMotion
import dev.morpho.ui.designsystem.theme.MorphoTheme

/**
 * The full-width pill for a screen's main action (Start, Continue, Done for today, Sync
 * now): ink, or outlined in mist for the secondary one; it dims while disabled and gives
 * way under the finger like every other control.
 */
@Composable
fun PrimaryButton(
    text: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    outlined: Boolean = false,
) {
    val primary = MaterialTheme.colorScheme.primary
    Button(
        onClick = onClick,
        enabled = enabled,
        shape = MorphoTheme.radii.shapeFull,
        colors = if (outlined) {
            ButtonDefaults.outlinedButtonColors(contentColor = primary)
        } else {
            ButtonDefaults.buttonColors(
                disabledContainerColor = primary.copy(alpha = DISABLED_ALPHA),
                disabledContentColor = MaterialTheme.colorScheme.onPrimary.copy(alpha = DISABLED_TEXT_ALPHA),
            )
        },
        border = if (outlined) BorderStroke(1.dp, MorphoTheme.accents.motifBase) else null,
        modifier = modifier
            .fillMaxWidth()
            .heightIn(min = MorphoTheme.sizes.primaryButton)
            .pressMotion(enabled),
    ) {
        Text(text = text, style = MaterialTheme.typography.titleSmall, textAlign = TextAlign.Center)
    }
}

private const val DISABLED_ALPHA = 0.4f
private const val DISABLED_TEXT_ALPHA = 0.8f
