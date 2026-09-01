---
file: app/app/src/main/kotlin/dev/morpho/ui/designsystem/component/Motif.kt
---

# Icon Mark System

The four marks below the M in the icon (square → diamond → diamond → diamond) are elevated from decoration to formal components. Read from left to right as "unlearned → learning → mastered" — the entire app's learning model is condensed into four shapes.

## MarkKind

Mark shapes: SQUARE or DIAMOND.

## MorphoMark(kind, color, modifier, size)

An individual icon mark that can be placed inline next to a label.

- **kind** — shape
- **color** — color
- **size** — size, default 10dp

## MOTIF_MARK_COUNT

The number of marks used by all wide progress rows/lines, fixed at 12. One mark is approximately 1/12 of the target.

## MotifProgressRow(fraction, modifier, markCount, cell, contentDescription)

Progress rows/lines drawn using icon marks.

- **fraction** — progress value from 0 to 1
- **markCount** — total mark count, default 12
- **cell** — cell size for each mark, default 14dp
- **contentDescription** — accessibility label
- Completed steps are solid diamonds; in progress rows/lines, the current step is a copper diamond (visual focus); unstarted steps are mist-blue squares; spacing is computed automatically to fill the parent container

## MotifMilestoneRail(fraction, modifier, milestones, contentDescription)

A thin rail-style progress item, suitable for long-arc scenarios such as vocabulary size. Milestones sit on the rail as diamonds.

- **fraction** — progress value from 0 to 1
- **milestones** — milestone position list, default [0.25, 0.5, 0.75]
- **contentDescription** — accessibility label
- Reached milestones are solid copper diamonds; unreached ones are outlined diamonds

## SectionHeading(text, modifier, trailing)

Section title rows/lines: all-caps label + thin line extending to the right edge + optional trailing control.

- **text** — title text
- **trailing** — optional control at the end of the title row/line

## MotifOrnament(modifier)

Centered decoration: thin line — copper diamond — thin line. Used at the end of a group of sections.

## MotifSignature(modifier, size)

A static arrangement of the icon's four marks — square, diamond, diamond, diamond. Used as a brand auxiliary identifier.

- **size** — size of each mark, default 8dp
