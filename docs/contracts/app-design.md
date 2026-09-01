# App Design System Contract (app/)

Motion, sound, and haptics are specified as one system rather than per screen. Everything below is normative; refine details freely but report deviations.

## Brand

### Colour

| Seed | Hex | Light theme | Dark theme |
|---|---|---|---|
| Ink blue | `#22314A` | text | ground |
| Copper gold | `#C08F4A` | accent | accent |
| Parchment | `#F2EDE2` | ground | text |
| Mist blue | `#6B7893` | secondary | secondary |

| Semantic | Hex | Use |
|---|---|---|
| Verdigris | `#3B7A67` | correct |
| Oxblood | `#9E322D` | error |

Static scheme, no Material You dynamic colour. Implementation: `ui/designsystem/theme/Color.kt` — `MorphoPalette` defines tonal ramps per seed, `MorphoLightColorScheme` / `MorphoDarkColorScheme` map to M3 roles, `MorphoAccents` carries semantic colours.

### Typography

- Display / headline / title: EB Garamond
- Content text (definitions, sentences, quiz options, etymology): system sans-serif
- Phonetics: sans, alpha 0.7
- Type scale: M3 defaults, `displaySmall` for word headline, `bodyLarge` 1.5× line-height for definitions

Implementation: `ui/designsystem/theme/Type.kt` — `MorphoFonts.displayFontFamily` controls display face, `MorphoFonts.readingFontFamily` controls content face. `MorphoReadingTypography` defines per-content-type sizes: definition 16sp/24sp, sentence 22sp/34sp, phonetic 15sp/20sp.

## Component inventory (`ui/designsystem/`)

Tokens (`ui/designsystem/token/`): spacing (4dp grid), radii (12/16/24), elevation, durations, easings — no magic numbers in screens.

Components (`ui/designsystem/component/`), each with `@Preview` in both themes:

- `WordHeader` — word + IPA + `AudioChipButton` (pulsing ring while playing)
- `SentenceCard` — sentence (sans, reading style), target span highlighted (primary-container pill background), tap-to-play audio
- `QuizImageGrid` — 2×2 images, states: idle / pressed (scale .97) / correct (blue ring + check badge) / wrong (shake + dim); min touch target 48dp
- `QuizImageDefGrid` — image + one-line definition caption per cell (mode 2)
- `QuizTextOptions` — 4 stacked definition cards (mode 3)
- `DefinitionBlock` — pos chip + definition text (sans, reading style) + audio button
- `ModePips` — 3-dot mode-level indicator with animated fill on promotion
- `GroupProgressBar` — segmented per-word progress within group
- `ProgressRing` — animated sweep, center stats (home screen)
- `StreakBadge`, `StatTile`, `SessionSummaryCard`
- `SpellInput` — letter boxes for listening-spell, per-char reveal animation
- `AnswerFeedbackOverlay` — full-screen brief flash layer (correct/wrong), hosts particle/Lottie effects
- `DetailSheet` — word detail: header, definitions by pos, examples, etymology breakdown (`EtymologyChips`: root segments as connected chips)

## Motion spec

M3 motion system; standard easing `CubicBezier(0.2, 0, 0, 1)`.

| Interaction | Spec |
|---|---|
| Option press | scale 1→0.97, 100 ms |
| Correct answer | ring + check badge spring (`Spring.DampingRatioMediumBouncy`), 250 ms; other options fade to 0.4 |
| Wrong answer | horizontal shake ±8dp spring, 300 ms; red flash on `AnswerFeedbackOverlay` 150 ms |
| Question transition | shared-axis X (slide 30dp + fade), 300 ms |
| Mode promotion | `ModePips` fill + subtle glow, 400 ms |
| Group complete | Lottie confetti/butterfly burst ≤1.5 s, skippable by tap |
| Progress ring | animated sweep 600 ms decelerate, on home resume |
| Detail sheet | M3 modal bottom sheet default motion |

Respect reduced-motion: scale/shake replaced by opacity when the system animator duration scale is 0 (`Settings.Global.ANIMATOR_DURATION_SCALE` — Compose exposes no first-class signal), plus a manual override in Settings.

## Sound design

Two channels, independent volumes: content audio (word/def/example TTS via Media3) and UI SFX (SoundPool, ≤150 ms latency, ogg/wav ≤100 KB each). SFX events:

| Event | Character |
|---|---|
| `tap` | soft tick, barely-there |
| `correct` | short marimba ding, upward |
| `wrong` | muted low thud (never harsh) |
| `promote` | two-note rise |
| `group_complete` | 3-note fanfare |
| `review_done` | soft chime |
| `streak` | sparkle |

`SoundManager` (data layer): preloads all SFX at startup, exposes `play(SfxEvent)`, global mute + volume in settings, never plays SFX over content audio at full volume (duck SFX to 0.6 while TTS active). Wave 1 ships **generated placeholder tones** (write a small Python script `app/tools/gen_sfx.py` synthesizing sine/decay envelopes to wav — no downloads); real sound design swaps files later.

## Haptics map

`HapticsManager` wrapping `HapticFeedbackConstants` / `VibrationEffect`: correct = light tick; wrong = double buzz (2×40 ms); promote = tick-tick; group complete = success pattern. Global toggle in settings. Always paired with visual feedback, never haptic-only.

## Screens (wave 1)

`Home` (progress ring, today's task card, streak, start CTA) · `Learn` (modes 1/2/3 + feedback + detail sheet on wrong answer / word completion) · `Review` (definition→word, listening-spell) · `Session summary` · `Settings` (daily goal, sound/haptics toggles, backup export/import stub). Navigation: single-activity, `androidx.navigation.compose`.

## Accessibility

Touch targets ≥48dp; contrast AA both themes; content descriptions on all interactive elements; TalkBack order sane on quiz grids.
