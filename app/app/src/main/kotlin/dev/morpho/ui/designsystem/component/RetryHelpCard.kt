package dev.morpho.ui.designsystem.component

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import dev.morpho.ui.designsystem.theme.MorphoTheme
import dev.morpho.ui.designsystem.theme.PHONETIC_ALPHA

/**
 * The tallest the card is allowed to grow before its content starts scrolling inside it.
 *
 * A miss card lives in the [QuizLayout] `banner` slot, wedged between the prompt and the
 * bottom-anchored answer grid. The grid's height is derived from the viewport, not from
 * its siblings, so a growing banner compresses the (scrollable) prompt above it — but only
 * up to a point: left unbounded on a short screen the card would out-measure the whole
 * middle band and paint over the grid. Capping the height and scrolling the overflow keeps
 * the grid where the thumb last tapped it. The cap is also clamped by the space the layout
 * actually grants the banner, so on a short device it shrinks below this value.
 */
private val RETRY_CARD_MAX_HEIGHT = 300.dp

/**
 * The wrong-answer help card: the teachable moment made rich.
 *
 * A bare "try again" wastes the one instant the learner is most receptive. So on a miss the
 * card names the word, shows how it is written and sounds (with its own play button), and
 * lays out *every* selected sense — part-of-speech chip and definition — so form and meaning
 * land together before the next pick. The highlighted example is already on screen above, so
 * it is not repeated here.
 *
 * It paints entirely inside the error-container palette; this is a wrong-answer surface and a
 * second accent hue would read as an affordance rather than as a warning. Definitions are
 * plain text on purpose: the anchor-gloss trigger is exactly that stray third hue, and the
 * same glossed definitions are one tap away on the detail sheet that opens after the correct
 * retry — so nothing is lost by leaving them un-glossed here.
 */
@Composable
fun RetryHelpCard(
    hint: String,
    word: String,
    phonetic: String?,
    senses: List<SenseDetail>,
    onPlayWord: () -> Unit,
    modifier: Modifier = Modifier,
    playing: Boolean = false,
    maxHeight: Dp = RETRY_CARD_MAX_HEIGHT,
) {
    val spacing = MorphoTheme.spacing
    val onError = MaterialTheme.colorScheme.onErrorContainer
    Column(
        modifier = modifier
            .fillMaxWidth()
            .clip(MorphoTheme.radii.shapeMd)
            .background(MaterialTheme.colorScheme.errorContainer)
            .heightIn(max = maxHeight)
            .verticalScroll(rememberScrollState())
            .padding(spacing.md),
        verticalArrangement = Arrangement.spacedBy(spacing.sm),
    ) {
        Text(
            text = hint,
            style = MaterialTheme.typography.labelLarge,
            color = onError,
        )

        Row(
            modifier = Modifier.fillMaxWidth(),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(spacing.sm),
        ) {
            Column(
                modifier = Modifier.weight(1f),
                verticalArrangement = Arrangement.spacedBy(spacing.xxs),
            ) {
                Text(
                    text = word,
                    // Sans, not the chrome-reserved titleLarge face (#34): the missed
                    // word is content the learner reads, same as WordHeader's headline.
                    style = MaterialTheme.typography.titleLarge.copy(
                        fontFamily = MorphoTheme.reading.wordHeadline.fontFamily,
                    ),
                    color = onError,
                )
                if (!phonetic.isNullOrBlank()) {
                    Text(
                        text = phonetic,
                        style = MorphoTheme.reading.phonetic,
                        color = onError.copy(alpha = PHONETIC_ALPHA),
                        modifier = Modifier.clearAndSetSemantics {
                            contentDescription = "Pronunciation $phonetic"
                        },
                    )
                }
            }
            AudioChipButton(
                onClick = onPlayWord,
                playing = playing,
                small = true,
                contentDescription = "Play pronunciation of $word",
            )
        }

        // Every selected sense, primary first or not — the same set the detail sheet lists,
        // minus its per-sense audio (one word-level play button is enough for an error card).
        Column(verticalArrangement = Arrangement.spacedBy(spacing.sm)) {
            senses.forEach { sense ->
                RetrySense(sense = sense, contentColor = onError)
            }
        }
    }
}

@Composable
private fun RetrySense(sense: SenseDetail, contentColor: Color) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .semantics { contentDescription = "${sense.pos}. ${sense.definition}" },
        verticalArrangement = Arrangement.spacedBy(MorphoTheme.spacing.xxs),
    ) {
        Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(MorphoTheme.spacing.xs),
        ) {
            Text(
                text = sense.pos,
                style = MaterialTheme.typography.labelSmall,
                color = contentColor,
                modifier = Modifier
                    .clip(MorphoTheme.radii.shapeXs)
                    .background(contentColor.copy(alpha = 0.14f))
                    .padding(
                        horizontal = MorphoTheme.spacing.xs,
                        vertical = MorphoTheme.spacing.xxs,
                    ),
            )
            if (sense.isPrimary) {
                Text(
                    text = "primary",
                    style = MaterialTheme.typography.labelSmall,
                    color = contentColor.copy(alpha = PHONETIC_ALPHA),
                )
            }
        }
        Text(
            text = sense.definition,
            style = MorphoTheme.reading.definition,
            color = contentColor,
        )
    }
}

private val previewSenses = listOf(
    SenseDetail(
        "adj",
        "kind and generous towards other people, especially those with less power",
        true,
        "audio/benevolent-def1.ogg",
    ),
    SenseDetail(
        "adj",
        "wishing to do good and to help, rather than to gain something",
        false,
        "audio/benevolent-def2.ogg",
    ),
)

@ThemePreviews
@Composable
private fun RetryHelpCardPreview() {
    PreviewBox {
        RetryHelpCard(
            hint = "Not this one. Read the meaning, then pick again.",
            word = "benevolent",
            phonetic = "/bəˈnevələnt/",
            senses = previewSenses,
            onPlayWord = {},
        )
    }
}
