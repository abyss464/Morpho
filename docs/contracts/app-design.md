# App Design System Contract (app/)

Commercial-grade bar: the app must feel like a polished consumer product on first run — motion, sound, and haptics designed as a system, not sprinkled. Everything below is normative; refine details freely but report deviations.

## Brand

Morpho = the blue morpho butterfly. Identity: **electric morpho blue on deep neutrals**, wings/flutter as the celebration motif (learning = metamorphosis).

- Primary: `#2461FF` (morpho blue); container tints derived per Material 3 tonal roles
- Secondary: `#00B8A9` (teal, correctness accents); Error: `#E5484D`
- Dark theme is first-class, both themes ship in wave 1. Use M3 `ColorScheme` (static branded scheme, not dynamic color).
- Typography: UI text `FontFamily.SansSerif`; **definitions & example sentences use `FontFamily.Serif`** (dictionary feel, long-form readability). Type scale: M3 defaults with `displaySmall` for the word headline, `bodyLarge` 1.5 line-height for definitions. Phonetics (IPA) in sans, `alpha 0.7`. Bundled custom fonts (Inter + Literata) are a later wave — build the Typography layer so swapping is one file.

## Component inventory (`ui/designsystem/`)

Tokens (`ui/designsystem/token/`): spacing (4dp grid), radii (12/16/24), elevation, durations, easings — no magic numbers in screens.

Components (`ui/designsystem/component/`), each with `@Preview` in both themes:

- `WordHeader` — word + IPA + `AudioChipButton` (pulsing ring while playing)
- `SentenceCard` — serif sentence, target span highlighted (primary-container pill background), tap-to-play audio
- `QuizImageGrid` — 2×2 images, states: idle / pressed (scale .97) / correct (blue ring + check badge) / wrong (shake + dim); min touch target 48dp
- `QuizImageDefGrid` — image + one-line serif definition caption per cell (mode 2)
- `QuizTextOptions` — 4 stacked definition cards (mode 3)
- `DefinitionBlock` — pos chip + serif text + audio button
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

Respect reduced-motion: scale/shake replaced by opacity when `LocalAccessibilityManager` signals.

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
