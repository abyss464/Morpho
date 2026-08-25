package dev.morpho.ui.designsystem.component

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.text.style.TextAlign
import dev.morpho.ui.designsystem.theme.MorphoTheme
import dev.morpho.ui.designsystem.theme.PHONETIC_ALPHA

/**
 * Word + IPA + play button. The headline uses `displaySmall`-scale type; the IPA is
 * sans at alpha 0.7 (docs/contracts/app-design.md, "Brand").
 */
@Composable
fun WordHeader(
    word: String,
    phonetic: String?,
    onPlayAudio: () -> Unit,
    modifier: Modifier = Modifier,
    playing: Boolean = false,
    centered: Boolean = true,
) {
    val alignment = if (centered) Alignment.CenterHorizontally else Alignment.Start
    Column(
        modifier = modifier.fillMaxWidth(),
        horizontalAlignment = alignment,
        verticalArrangement = Arrangement.spacedBy(MorphoTheme.spacing.xxs),
    ) {
        Text(
            text = word,
            style = MorphoTheme.reading.wordHeadline,
            color = MaterialTheme.colorScheme.onSurface,
            textAlign = if (centered) TextAlign.Center else TextAlign.Start,
        )
        Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(MorphoTheme.spacing.xs),
        ) {
            if (!phonetic.isNullOrBlank()) {
                Text(
                    text = phonetic,
                    style = MorphoTheme.reading.phonetic,
                    color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = PHONETIC_ALPHA),
                    modifier = Modifier.clearAndSetSemantics {
                        contentDescription = "Pronunciation $phonetic"
                    },
                )
            }
            AudioChipButton(
                onClick = onPlayAudio,
                playing = playing,
                small = true,
                contentDescription = "Play pronunciation of $word",
            )
        }
    }
}

@ThemePreviews
@Composable
private fun WordHeaderPreview() {
    PreviewBox {
        Column(verticalArrangement = Arrangement.spacedBy(MorphoTheme.spacing.lg)) {
            WordHeader(word = "benevolent", phonetic = "/bəˈnevələnt/", onPlayAudio = {})
            WordHeader(
                word = "metamorphosis",
                phonetic = "/ˌmetəˈmɔːfəsɪs/",
                onPlayAudio = {},
                playing = true,
                centered = false,
            )
        }
    }
}
