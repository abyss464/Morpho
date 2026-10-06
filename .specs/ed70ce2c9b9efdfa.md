---
file: app/domain/src/main/kotlin/dev/morpho/domain/stream/Spell.kt
---

# Spelling

The spelling that ends a rebuild review (docs/contracts/stream.md §2). A faithful port of the web client's `buildSpell`.

## Tile / SpellPuzzle
- **Tile** — an id and its letter
- **SpellPuzzle** — per character of the word, the character when given (a hint letter, a space, a hyphen) or null for a blank; the letters the blanks take, in order; and the tiles on offer, shuffled

## Spelling.isLetter(character)
Whether a character is a letter, as JavaScript's `\p{L}`.

## Spelling.puzzle(word, id) -> SpellPuzzle
The lowercased word: spaces and hyphens are given, so is the first letter, and the last one too when the word has more than 5 letters. The tiles are the missing letters plus 2 decoys from "etaoinshrdlucmfwypbgvk" not in the word. Shuffled with Seeded(id + 13): the decoy candidates first, then the tiles.
