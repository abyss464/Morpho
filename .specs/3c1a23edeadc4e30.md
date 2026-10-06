---
file: app/app/src/main/kotlin/dev/morpho/ui/today/JourneyProgress.kt
---

# Journey Progress

How much of the whole release the learner has met, as a card.

## JourneyProgress(overall, estimatedDaysRemaining, modifier)

- overall: words in the release, words met so far (a card, or Learning in the stream), words being learned now
- estimatedDaysRemaining: days left at the recent pace of new words, or null

### Card content

1. **Section title** "Journey"
2. **Main figure** — words met, in large Garamond, with "of N words" beside it
3. **Milestone rail** — quarter marks as the icon's diamonds: solid copper once passed, outlined while ahead
4. **Footer** — the share met on the left; on the right a completion mark when every word is met, "~N days left" when an estimate exists, or nothing
