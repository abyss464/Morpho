---
file: app/app/src/main/kotlin/dev/morpho/ui/designsystem/theme/Color.kt
---

# Brand Palette and Color Schemes

Static brand colors — does not use Material You dynamic color extraction. All colors derive from the four seed colors of the app icon.

## Palette (MorphoPalette)

Four tonal scales, each with multiple steps from dark to light:

- **Ink** — The icon's base color. Text color in light themes, background color in dark themes.
- **Mist** — The color of the static blocks in the icon. Used as a secondary color.
- **Copper** — The color of the active diamond in the icon. The only accent color.
- **Parchment** — The color of the paper sheet in the icon. Background color in light themes, text color in dark themes.

Two additional semantic tonal scales:

- **Verdigris** — Correctness signal, derived from the oxidation color of copper.
- **Oxblood** — Error signal, derived from bookbinding leather.

## Light Scheme (MorphoLightColorScheme)

Parchment for the background, Ink for text, Copper as the focal point. Cards are lighter than the page, as if a sheet of paper resting on a table.

## Dark Scheme (MorphoDarkColorScheme)

Ink for the background, Parchment for text, Copper's role unchanged.

## Semantic Accents (MorphoAccents)

Material roles are overridden with brand semantic colors, accessible via `MorphoTheme.accents`:

- correct / wrong — correct, wrong
- streak — winning streak
- highlight / onHighlight — highlight
- ringTrack — progress ring track
- modePipInactive — inactive state of the mode indicator
- shimmer — shimmer overlay
- motifBase / motifActive / motifMastered — three icon progress states: not started (Mist), learning (Copper), mastered (Ink or Parchment)
- rule — section divider line

Light and dark themes each have a preset set of values (LightAccents / DarkAccents).
