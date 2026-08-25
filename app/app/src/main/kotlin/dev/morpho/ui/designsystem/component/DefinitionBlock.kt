package dev.morpho.ui.designsystem.component

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import dev.morpho.ui.designsystem.theme.MorphoTheme

/**
 * One sense: part-of-speech chip, serif definition text, and its own play button.
 * The detail sheet stacks one of these per selected sense.
 */
@Composable
fun DefinitionBlock(
    pos: String,
    definition: String,
    onPlayAudio: () -> Unit,
    modifier: Modifier = Modifier,
    playing: Boolean = false,
    isPrimary: Boolean = false,
) {
    Row(
        modifier = modifier
            .fillMaxWidth()
            .semantics { contentDescription = "$pos. $definition" },
        verticalAlignment = Alignment.Top,
        horizontalArrangement = Arrangement.spacedBy(MorphoTheme.spacing.sm),
    ) {
        Column(
            modifier = Modifier.weight(1f),
            verticalArrangement = Arrangement.spacedBy(MorphoTheme.spacing.xxs),
        ) {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(MorphoTheme.spacing.xs),
            ) {
                PosChip(pos)
                if (isPrimary) {
                    Text(
                        text = "primary",
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.primary,
                    )
                }
            }
            Text(
                text = definition,
                style = MorphoTheme.reading.definition,
                color = MaterialTheme.colorScheme.onSurface,
                modifier = Modifier.padding(top = MorphoTheme.spacing.xxs),
            )
        }
        AudioChipButton(
            onClick = onPlayAudio,
            playing = playing,
            small = true,
            contentDescription = "Play this definition",
        )
    }
}

@ThemePreviews
@Composable
private fun DefinitionBlockPreview() {
    PreviewBox {
        Column(verticalArrangement = Arrangement.spacedBy(MorphoTheme.spacing.lg)) {
            DefinitionBlock(
                pos = "adj",
                definition = "kind and generous towards other people, especially those with less power",
                onPlayAudio = {},
                isPrimary = true,
            )
            DefinitionBlock(
                pos = "noun",
                definition = "a person or group that gives help, money or support",
                onPlayAudio = {},
                playing = true,
            )
        }
    }
}
