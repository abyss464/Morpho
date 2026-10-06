---
file: app/app/src/main/kotlin/dev/morpho/ui/stream/SpellTask.kt
---

# Spell Task

The spelling screen's controls (docs/contracts/stream.md §2).

## MaskedDefinition(word, modifier)
The primary definition with every form of the word replaced by a short copper rule, so it names nothing.

## SpellBoard(state, onReturn, modifier)
The word as letter cells: given letters on the panel in muted ink, open blanks dashed (the next one with a copper border), filled blanks as letters that go back when tapped. After a failed check the wrong letters turn red and the row shakes once; once solved they turn green. The status: "Spell the word. Two of the letters are not in it.", "Not quite. Tap the red letters to take them back." or "That's the word."

## LetterTiles(state, onPlace, modifier)
The letter tiles; a tile in the word keeps its slot, empty. Hidden once solved.

## SpellActions(state, onShowNextLetter, onShowWord, onStartOver, modifier)
Until the word is right: "Show the next letter", "Show the word" and "Start over" (enabled once a blank is filled).
