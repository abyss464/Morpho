---
file: app/app/src/main/kotlin/dev/morpho/ui/designsystem/theme/Type.kt
---

# Typography System

Three faces, each with one job (docs/contracts/stream.md §8): EB Garamond for display, Source Serif 4 for what the learner reads as English, the system sans for interface text.

## Font Families (MorphoFonts)

- displayFontFamily — EB Garamond, the icon's face: the wordmark, titles, the headword, word options, figures. Bundled as a variable font.
- readingFontFamily — Source Serif 4: definitions, example sentences, definition pieces. Bundled as two static instances, regular and semibold.
- uiFontFamily — system sans-serif: labels, buttons, counts, IPA.

## UI Typography (MorphoTypography)

M3 Typography with the display / headline / title layers on EB Garamond and the body / label layers on the sans.

## Section Label (MorphoSectionLabel)

Small tracked sans label above each Today section and as the stream's step label.

## Reading Typography (MorphoReadingTypography)

- definition — the definition on a word card (Source Serif 4, 19sp)
- definitionCompact — a smaller definition: the review result, a wrong option's meaning
- sentence — the example under a definition
- gapSentence — the example with its gap on the use step
- piece — one definition piece in the tray and bank
- phonetic — IPA in sans (render at 0.7 opacity)
- wordHeadline — the headword on a word card (Garamond 40sp)
- prompt — a step's question ("What does … mean?", "Which word fits?")
- wordOption — a word as a fill-in option
- statNumber — figures on the done screen's tiles
- heroNumber — the headline figure on Today (words met)

## PHONETIC_ALPHA

Opacity constant for pronunciation, value 0.7.
