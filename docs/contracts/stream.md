# Stream Contract (web/ and app/)

One learning experience, two clients. The learner never chooses between "learn" and
"review": a single mixed **stream** decides what comes next. Both clients implement this
file; behaviour that differs between them is a defect unless this file names it as a
platform adaptation.

## 1. Stages of a word

Every word moves through four stages. A word is in exactly one stage at a time.

| Stage | Meaning | Steps it owns |
|---|---|---|
| **New** | not yet met | — |
| **Learning** | met today, not yet graduated | `know`, `explain1`, `explain2`, `use` |
| **Review** | graduated; scheduled by FSRS | `review` |
| **Relearning** | a review was rated Again | `know`, `explain2` |

Words enter in release learning order (`words.learning_order`), so an auxiliary word that a
definition depends on is always met before the word that uses it.

## 2. Steps

All steps are completed with taps or clicks only. A keyboard is never required.

### `know` — the word card
Picture, word, phonetic, part of speech, definition, example. On arrival the word, its
primary definition and its example are read aloud in that order. Always passes.

### `explain1` / `explain2` — rebuild the meaning
"What does *word* mean?" The primary definition is cut into pieces (§4); the learner taps
them in order into a tray. Decoy pieces from other words are mixed in.

- Placing the last piece checks the answer. Misplaced pieces turn red and go back to the bank
  when tapped. "Start over" clears the tray.
- After one failed check, "Show the next piece" appears: it keeps the correct opening pieces
  and places the next correct one.
- `explain1` comes **immediately** after `know`, shows the picture, uses easy pieces and
  decoys drawn from the words currently in the learning window.
- `explain2` comes later (§5), shows no picture, uses hard pieces and decoys drawn from all
  words already met.
- Solving reads the definition aloud. Continue is disabled until the tray is correct.

Outcome of an explain step:

| Result | Name |
|---|---|
| solved, no failed check, no hint | **clean** |
| solved after exactly one failed check, no hint | **shaky** |
| solved with a hint, or after two or more failed checks | **failed** |

### `use` — fill the word into its example
The example sentence with the word blanked; four word options: the word and its three bound
distractors (`distractors` table). Distractors the learner has already met are shown as they
are; the order of options is shuffled deterministically per word. Picking the right word
fills the blank and reads the sentence aloud. A wrong pick marks that option red, shows that
word's own primary definition under it, and the learner picks again.

Outcome: **clean** = right on the first pick; **failed** = otherwise.

### `review` — one task per due word
Shows the word only (picture behind a "Show picture" button) and plays the word. The task
type alternates with the word's review count: odd reviews (1st, 3rd, …) are an `explain2`
style rebuild; even reviews are a `use` style fill-in. After the task, the full card is shown
(picture, definition, example; definition read aloud) with the derived rating (§6), the next
interval, and a four-button control to change the rating. "Say it in your own words" is an
optional note, saved per word and shown on the word card afterwards.

## 3. Transitions

| Step and outcome | Next for that word |
|---|---|
| `know` (Learning) | `explain1` at once |
| `explain1` clean or shaky | `explain2` (delayed) |
| `explain1` failed | `explain2` (delayed), and it must be **clean** twice |
| `explain2` clean | `use` (delayed) |
| `explain2` shaky | another `explain2` (delayed) |
| `explain2` failed | `know`, then `explain2` (delayed) |
| `use` clean | **graduates**: enters Review |
| `use` failed | `explain2` (delayed), then `use` (delayed) |
| `review` rated Again | Relearning: `know` at once, then `explain2` (delayed) |
| Relearning `explain2` clean or shaky | back to Review (FSRS state already updated by the Again) |
| Relearning `explain2` failed | `know`, then `explain2` (delayed) |

Graduation rates the word's first FSRS review as **Good**, or **Hard** if any of its
learning steps was shaky or failed. Its first due date follows from that.

## 4. Pieces

A definition is split into clause-sized pieces:

1. Start a new piece before: that, who, which, where, when, whose, because, but, if, without,
   such, than, while, until, unless (only when the current piece has at least 2 words).
2. End a piece after a word ending in `,` `;` `:` (when it has at least 2 words).
3. The first piece splits right after its copula when one of is / are / means / mean is
   among its first five words ("A system is" | "a group of …").
4. A piece longer than MAX words splits nearest its middle, preferring before "or"/"and",
   then before a preposition (of in on at for from by with about into to over under).
5. One-word scraps merge into the previous piece; while there are more than 6 pieces, the
   adjacent pair with the fewest words merges.

MAX is 7 words for easy pieces and 4 for hard pieces. Decoys are pieces of other words'
primary definitions, never their first piece, never containing that word's headword, never
equal to a piece of the answer: 2 decoys (3 when the answer has 5 or more pieces) for easy,
3 (4 when 5 or more) for hard. Shuffles are deterministic per word id.

## 5. Mixing the stream

The stream is a list of steps chosen one at a time. Parameters: learning window **W = 5**
words, spacing **S = 3** steps, daily new words **N** (setting, default 20), backlog guard
**B = 50** due reviews.

To choose the next step:

1. **Delayed steps that are ready.** A delayed step is ready when at least S other steps have
   been taken since that word's last step. Take the ready one that has waited longest.
2. **Otherwise alternate review and new.** Take up to two due reviews (lowest FSRS
   retrievability first), then one new word (`know`). A new word is allowed only when fewer
   than W words are Learning or Relearning, fewer than N words were introduced today, and
   fewer than B reviews are due.
3. **Nothing ready but delayed steps pending.** Take the delayed step that has waited
   longest, even if not yet spaced.
4. **Nothing left.** The stream for today is done.

Two guards apply to every choice when an alternative exists: the same word never appears
in two consecutive steps (except `know` → `explain1`), and the same step type never appears
more than three times in a row.

Progress shown to the learner: steps done today out of done + remaining, where remaining is
due reviews + pending steps of Learning/Relearning words + 4 × new words still allowed today.

The learner can pause at any step; the stream resumes exactly where it stopped. "Meet 5 more
words" on the done screen raises today's new-word allowance by 5.

## 6. Ratings

Review ratings are derived from the task, never asked for first:

| Task result | Rating |
|---|---|
| clean and answered within 10 s (rebuild) / 4 s (fill-in) | Easy |
| clean | Good |
| shaky | Hard |
| failed | Again |

The derived rating is applied at once and shown with its next interval; tapping another
rating replaces it (the card is rescheduled from its state before this review).

## 7. Screens

- **Today** — the only entry: "Today", one line "R reviews and N new words", **Continue**.
  Below: journey (words met of total), this week, look up a word, units as progress only
  (20 words per unit in learning order; bar of graduated words).
- **Stream** — one frame for every step: pause, today's progress bar, steps done / total;
  the step label in the card's corner (New word, Explain it, Use it, Review).
- **Done** — reviewed count (clean count), words met (graduated first time), tomorrow's due
  count, streak; Done for today / Meet 5 more words.

Platform adaptations: web lays the card out picture | content side by side and accepts
arrow and number keys as shortcuts; the phone stacks content, keeps the piece bank and the
Continue button in the lower half, and adds haptics and sound effects per
`app-design.md`.

## 8. Look

Both clients use one palette and type set:

| Token | Value |
|---|---|
| ground | `#F2EDE2` |
| surface | `#FCFAF4` |
| sunken | `#E9E2D3` |
| ink | `#131C2A` |
| primary | `#22314A` |
| secondary text | `#414C64` |
| line | `#DED5C2`, strong `#C9BEA6` |
| accent (copper) | `#B07C33`, tint `#F4E4CD`, on tint `#4A3316` |
| good | `#3B7A67` on `#D8ECE3` |
| bad | `#9E322D` on `#F8DBD5` |
| new (mist) | `#6B7893` |

Display: EB Garamond. Definitions and sentences: Source Serif 4. Interface text: the
system sans. The design boards live in the "Unified stream" page of the Morpho UI canvas.
