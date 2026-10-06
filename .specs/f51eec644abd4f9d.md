---
file: app/app/src/main/kotlin/dev/morpho/ui/stream/StreamScreen.kt
---

# Stream Screen

One frame for every step (docs/contracts/stream.md §2, §7): pause, today's progress bar and "done / total" on top, the step in the middle, its answer area and action fixed at the foot. When nothing is left, the done screen.

## StreamScreen(container, onExit, modifier)

- container: the dependency container
- onExit: called on pause (the close button or system back) and on "Done for today"

### Steps

- **know** — the word card with "New word · Unit N" (or "Look again"). Continue is always enabled.
- **explain1 / explain2** — "Explain it", "What does *word* mean?" (with a picture thumbnail on `explain1` only), the definition with its blanks; the piece bank and its ways out sit above Continue, which opens once the tray is right.
- **use** — "Use it", the picture, "Which word fits?", the example with its blank, the meanings of wrongly picked words; the four options sit above Continue.
- **review** — "Review · Nth time", the picture covered behind "Show picture"; a rebuild shows "What does *word* mean?" with a play button, a fill shows the sentence only. Solving turns the step into its result.
- **review result** — picture, word, the verdict box (how the task went, around the definition), the example, the note, the rating with its next interval, the four-way rating control, and "Say it in your own words".

The answer area never moves while the learner works; content above it scrolls, and the frame pads for the keyboard.

## reviewLabel(reps)
"Review · Nth time", N being the card's review count before this review; "Review" without one.

## ordinal(n)
1 -> "1st", 2 -> "2nd", 11 -> "11th".
