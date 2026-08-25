package dev.morpho.ui.common

import dev.morpho.data.seed.EtymologyText
import dev.morpho.domain.model.WordBundle
import dev.morpho.ui.designsystem.component.ExampleDetail
import dev.morpho.ui.designsystem.component.SenseDetail
import dev.morpho.ui.designsystem.component.WordDetail

/**
 * Domain -> presentation mapping for the detail sheet.
 *
 * Byte offsets from release.db become Kotlin char ranges here, and the etymology
 * string is split into root chips plus prose (see [EtymologyText]).
 */
fun WordBundle.toWordDetail(): WordDetail {
    val etymology = EtymologyText.parse(word.etymology)
    return WordDetail(
        wordId = word.wordId,
        word = word.word,
        phonetic = word.phonetic,
        imageFile = word.imageFile,
        wordAudioFile = word.wordAudioFile,
        senses = senses.map {
            SenseDetail(
                pos = it.pos,
                definition = it.definition,
                isPrimary = it.isPrimary,
                audioFile = it.defAudioFile,
            )
        },
        examples = detailExamples.map {
            ExampleDetail(
                sentence = it.sentence,
                highlight = it.highlightCharRange(),
                audioFile = it.exAudioFile,
            )
        },
        etymology = etymology.prose,
        etymologySegments = etymology.segments,
    )
}
