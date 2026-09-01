---
file: app/app/src/main/kotlin/dev/morpho/ui/designsystem/component/QuizTextOptions.kt
---

# Text-Only Option List

Mode 3's answer interface: four vertically stacked definition cards, no images, no examples. Also serves the "definition-to-word" review type.

## TextOption

Data for a text option:

- **wordId** — word ID
- **text** — option text (definition or word)
- **pos** — optional part of speech abbreviation, displayed as a small tag
- **serif** — whether to use a serif font (definition options: true; word options: false)

## QuizTextOptions(options, onSelect, modifier, selectedIndex, correctIndex, revealed, enabled)

Vertically stacked text option card list.

- **options** — list of options
- **onSelect** — callback for selecting an option; takes the option index as a parameter
- **selectedIndex** — index of the option the user has already selected
- **correctIndex** — index of the correct answer
- **revealed** — whether the answer has already been revealed
- **enabled** — whether it is interactive
- Interaction animations match the image grid: scale down on press, border pop on the correct answer, shake on the wrong answer
- The gloss anchor in the card is triggered via long press (click = select answer)

## PosChip(pos, modifier)

Part of speech tag pill, shared by the answer card and the detail page. Displays the part of speech abbreviation text with a secondaryContainer-colored background.
