package dev.morpho.ui.designsystem.component

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.unit.dp
import dev.morpho.ui.designsystem.theme.MorphoTheme

/** Everything the detail sheet needs, already resolved from the release DB. */
data class WordDetail(
    val wordId: Long,
    val word: String,
    val phonetic: String?,
    val imageFile: String,
    val wordAudioFile: String,
    val senses: List<SenseDetail>,
    val examples: List<ExampleDetail>,
    val etymology: String?,
    val etymologySegments: List<String> = emptyList(),
)

data class SenseDetail(
    val pos: String,
    val definition: String,
    val isPrimary: Boolean,
    val audioFile: String,
)

data class ExampleDetail(
    val sentence: String,
    val highlight: IntRange,
    val audioFile: String,
)

/**
 * Word detail: header, image, every selected sense with its own audio, all examples,
 * and the etymology broken into root chips.
 *
 * Shown after a wrong answer (mandatory, per README Part 1) and after a word
 * graduates. Uses the default M3 modal bottom sheet motion.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun DetailSheet(
    detail: WordDetail,
    onDismiss: () -> Unit,
    onPlay: (String) -> Unit,
    modifier: Modifier = Modifier,
    playingFile: String? = null,
    continueLabel: String? = null,
    onContinue: (() -> Unit)? = null,
) {
    val sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = false)
    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = sheetState,
        modifier = modifier,
        containerColor = MaterialTheme.colorScheme.surfaceContainerLow,
    ) {
        DetailSheetContent(
            detail = detail,
            onPlay = onPlay,
            playingFile = playingFile,
            continueLabel = continueLabel,
            onContinue = onContinue,
        )
    }
}

/** The sheet body, split out so it can be previewed without a sheet host. */
@Composable
fun DetailSheetContent(
    detail: WordDetail,
    onPlay: (String) -> Unit,
    modifier: Modifier = Modifier,
    playingFile: String? = null,
    continueLabel: String? = null,
    onContinue: (() -> Unit)? = null,
) {
    val spacing = MorphoTheme.spacing
    Column(
        modifier = modifier
            .fillMaxWidth()
            .verticalScroll(rememberScrollState())
            .padding(horizontal = spacing.lg)
            .padding(bottom = spacing.xl)
            .navigationBarsPadding(),
        verticalArrangement = Arrangement.spacedBy(spacing.lg),
    ) {
        WordHeader(
            word = detail.word,
            phonetic = detail.phonetic,
            onPlayAudio = { onPlay(detail.wordAudioFile) },
            playing = playingFile == detail.wordAudioFile,
        )

        ContentImage(
            file = detail.imageFile,
            contentDescription = "Illustration for ${detail.word}",
            modifier = Modifier
                .fillMaxWidth()
                .aspectRatio(4f / 3f)
                .clip(MorphoTheme.radii.shapeMd),
        )

        SectionLabel("Definitions")
        Column(verticalArrangement = Arrangement.spacedBy(spacing.md)) {
            detail.senses.forEach { sense ->
                DefinitionBlock(
                    pos = sense.pos,
                    definition = sense.definition,
                    onPlayAudio = { onPlay(sense.audioFile) },
                    playing = playingFile == sense.audioFile,
                    isPrimary = sense.isPrimary,
                )
            }
        }

        if (detail.examples.isNotEmpty()) {
            HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)
            SectionLabel("Examples")
            Column(verticalArrangement = Arrangement.spacedBy(spacing.sm)) {
                detail.examples.forEach { example ->
                    SentenceCard(
                        sentence = example.sentence,
                        highlight = example.highlight,
                        onPlayAudio = { onPlay(example.audioFile) },
                        playing = playingFile == example.audioFile,
                        compact = true,
                    )
                }
            }
        }

        if (!detail.etymology.isNullOrBlank() || detail.etymologySegments.isNotEmpty()) {
            HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)
            SectionLabel("Word origin")
            if (detail.etymologySegments.isNotEmpty()) {
                EtymologyChips(detail.etymologySegments)
            }
            if (!detail.etymology.isNullOrBlank()) {
                Text(
                    text = detail.etymology,
                    style = MorphoTheme.reading.etymology,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }

        if (onContinue != null && continueLabel != null) {
            Button(
                onClick = onContinue,
                modifier = Modifier.fillMaxWidth(),
                shape = MorphoTheme.radii.shapeLg,
            ) {
                Text(continueLabel)
            }
        }
    }
}

/** Root/affix segments rendered as connected chips: `bene` + `vol` + `ent`. */
@Composable
fun EtymologyChips(segments: List<String>, modifier: Modifier = Modifier) {
    FlowRow(
        modifier = modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.spacedBy(MorphoTheme.spacing.xxs),
        verticalArrangement = Arrangement.spacedBy(MorphoTheme.spacing.xs),
    ) {
        segments.forEachIndexed { index, segment ->
            Text(
                text = segment,
                style = MaterialTheme.typography.labelLarge,
                color = MaterialTheme.colorScheme.onPrimaryContainer,
                modifier = Modifier
                    .clip(MorphoTheme.radii.shapeXs)
                    .background(MaterialTheme.colorScheme.primaryContainer)
                    .padding(
                        horizontal = MorphoTheme.spacing.sm,
                        vertical = MorphoTheme.spacing.xs,
                    ),
            )
            if (index != segments.lastIndex) {
                Box(
                    Modifier
                        .align(Alignment.CenterVertically)
                        .padding(horizontal = 2.dp),
                ) {
                    Text(
                        text = "+",
                        style = MaterialTheme.typography.labelLarge,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
        }
    }
}

@Composable
private fun SectionLabel(text: String) {
    Text(
        text = text,
        style = MaterialTheme.typography.titleSmall,
        color = MaterialTheme.colorScheme.primary,
    )
}

internal val previewDetail = WordDetail(
    wordId = 1,
    word = "benevolent",
    phonetic = "/bəˈnevələnt/",
    imageFile = "img/benevolent.webp",
    wordAudioFile = "audio/benevolent-word.ogg",
    senses = listOf(
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
    ),
    examples = listOf(
        ExampleDetail(
            "A benevolent stranger paid for the whole table without saying a word.",
            2..11,
            "audio/benevolent-ex1.ogg",
        ),
        ExampleDetail(
            "The fund relies on a benevolent donor who prefers to stay unknown.",
            20..29,
            "audio/benevolent-ex2.ogg",
        ),
    ),
    etymology = "From Latin bene 'well' and volens 'wishing' — literally well-wishing.",
    etymologySegments = listOf("bene", "vol", "ent"),
)

@ThemePreviews
@Composable
private fun DetailSheetContentPreview() {
    PreviewBox {
        DetailSheetContent(
            detail = previewDetail,
            onPlay = {},
            continueLabel = "Got it",
            onContinue = {},
        )
    }
}

@ThemePreviews
@Composable
private fun EtymologyChipsPreview() {
    PreviewBox {
        EtymologyChips(listOf("meta", "morph", "osis"))
    }
}
