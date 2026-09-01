---
file: app/app/src/main/kotlin/dev/morpho/ui/designsystem/component/RetryHelpCard.kt
---

# retry help card

When the user answers incorrectly, a teaching card pops up, using the easiest-to-accept moment for learners to convey complete information instantly. It is displayed directly above the answer area (the QuizLayout's banner slot), using the error container's color scheme. When the content exceeds the maximum height, scroll internally.

## RetryHelpCard(hint, word, phonetic, senses, onPlayWord, modifier, playing, maxHeight)

- **hint** — hint text (e.g., "Not this one. Read the meaning, then pick again.")
- **word** — the target word
- **phonetic** — pronunciation, nullable
- **senses** — the full list of selected definitions for this word (SenseDetail list), with part-of-speech tags; the primary sense has a "primary" marker
- **onPlayWord** — callback for playing the word's pronunciation
- **playing** — whether the pronunciation is currently playing
- **maxHeight** — the card's maximum height, default 300dp; scrolls internally when exceeded
- The card as a whole uses the errorContainer color scheme, and the definitions are rendered as plain text (without gloss anchors — the same definitions can be found in the detail page with links; after an incorrect answer, the detail page is forcibly shown)
