---
file: app/app/src/main/kotlin/dev/morpho/ui/designsystem/component/Stats.kt
---

# Statistics Components

Statistics display components used on the home page and learning summary page.

## StreakBadge(days, modifier)

Badge showing consecutive learning days, with warm colors to convey a sense of reward.

- **days** — Number of consecutive days
- Displays a flame icon + the day count + "day"/"days"

## StatTile(value, label, modifier, icon, emphasis)

A single data tile: a large number + descriptive text, with an optional icon.

- **value** — Numeric text (e.g., "18", "94%")
- **label** — Descriptive text (e.g., "new words", "accuracy")
- **icon** — Optional icon at the top
- **emphasis** — Whether to use an emphasized style (uses primaryContainer background)

## SessionSummaryCard(headline, supporting, modifier, content)

Summary card shown when learning/review ends.

- **headline** — Title (e.g., "Group cleared")
- **supporting** — Subtitle/descriptive text
- **content** — Additional content slot inside the card

## IconPill(icon, contentDescription, modifier)

Small circular icon badge used in list rows/lines and title areas. Circular primaryContainer background with a centered icon.
