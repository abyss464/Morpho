---
file: app/app/src/main/kotlin/dev/morpho/ui/stream/FillTask.kt
---

# Fill Task

The use step's controls (docs/contracts/stream.md §2).

## GapSentence(state, modifier)
The example with a copper rule where the word goes; once solved the word sits on the rule in semibold.

## WordOptions(state, onPick, modifier)
The four words, two per row, in Garamond. A wrong pick turns red, shakes once and stays disabled; the right pick turns green. The status line sits under the grid.

## WrongMeanings(state, modifier)
Each wrongly picked word with its own primary definition, shown under the sentence so the options never move.
