---
file: app/app/src/main/kotlin/dev/morpho/ui/designsystem/token/Spacing.kt
---

# Spacing tokens

4dp grid spacing system, accessed via `MorphoTheme.spacing`. The interface forbids using bare dp values:

- none (0) — no spacing
- xxs (4) — internal gaps within components
- xs (8) — between closely related elements
- sm (12) — padding within small components
- md (16) — default screen margin and card padding
- lg (20) — grid gap between quiz cells
- xl (24) — between sections
- xxl (32) — above primary action buttons
- xxxl (48) — large whitespace

Two other derived values:

- minTouchTarget (48) — minimum touch target required for accessibility
- screenGutter — default screen horizontal margin (equals md)
