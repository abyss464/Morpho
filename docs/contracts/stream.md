# Stream Contract (web/ and app/)

One learning experience, two clients. The learner never chooses between "learn" and
"review": a single mixed **stream** decides what comes next. Both clients implement this
file; behaviour that differs between them is a defect unless this file names it as a
platform adaptation.

## 1. Stages of a word

Every word moves through three stages. A word is in exactly one stage at a time.

| Stage | Meaning | Steps it owns |
|---|---|---|
| **New** | not yet met | — |
| **Learning** | met today, not yet graduated | `know`, `explain`, `spell`, `use` |
| **Review** | graduated; scheduled by FSRS | `review` |

A new word is learned in one pass: each step is passed once (§3). Its first review is the next
day.

Words enter in release learning order (`words.learning_order`), so an auxiliary word that a
definition depends on is always met before the word that uses it.

## 2. Steps

All steps are completed with taps or clicks only. A keyboard is never required.

### `know` — the word card
Picture, word, phonetic, part of speech, definition, example. On arrival the word, its
primary definition and its example are read aloud in that order. Always passes.

### Mistakes and passing
Every task counts the learner's mistakes:
- each blank found wrong at a check counts once (a wrong piece or letter left in its blank
  and checked again is not counted again);
- in `use` and fill-in reviews, each wrong pick counts once;
- "Show the next piece" / "Show the next letter" counts once per use;
- "Show the answer" / "Show the word" makes the task **failed** outright.

| Mistakes | Outcome | Passed |
|---|---|---|
| 0 | **clean** | yes |
| 1 | **shaky** | yes |
| 2 or more, or the answer shown | **failed** | no |

### `explain` — rebuild the meaning
"What does *word* mean?" The primary definition stands in the tray with blanks (§4): the
part that names the word, its prepositions and its punctuation are already in place, and the
learner taps pieces into the blanks. Decoy pieces from other words are mixed in.

- A tapped piece fills the first open blank. Filling the last blank checks the answer. Wrong
  pieces turn red and go back to the bank when tapped, reopening their blank. "Start over"
  empties every blank.
- Two ways out are always there: "Show the next piece" puts the right piece into the first
  blank that is open or wrong; "Show the answer" fills every blank.
- Comes **immediately** after `know`, shows the picture, uses easy pieces and decoys drawn
  from the words currently in the learning window.
- Solving reads the definition aloud. Continue is disabled until the tray is correct.

### `spell` — spell the word from its meaning
Comes **immediately** after a passed `explain`. The same task as a review's spelling (below):
"Which word means this?" over the definition with the word blanked out, letter tiles with two
decoys, the picture behind "Show picture". Solving plays the word.

### `use` — fill the word into its example
The example sentence with the word blanked; four word options: the word and its three bound
distractors (`distractors` table). Distractors the learner has already met are shown as they
are; the order of options is shuffled deterministically per word. Picking the right word
fills the blank and reads the sentence aloud. A wrong pick marks that option red, shows that
word's own primary definition under it, and the learner picks again. Comes after the word's
`spell`, delayed (§5).

### `review` — one task per due word
Shows the word only (picture behind a "Show picture" button) and plays the word. The task
type alternates with the word's review count: odd reviews (1st, 3rd, …) are a rebuild
(hard pieces, no picture, decoys from all words already met) followed by **spelling the
word**; even reviews are a `use` style fill-in.

Spelling: "Which word means this?" over the primary definition with every form of the word
blanked out; the picture stays behind "Show picture" and the word is not played. The word's
letters are blanks, except spaces, hyphens and hint letters: the first letter, and the last
too when the word has more than five letters. The tiles are the missing letters plus two
decoy letters the word does not contain, shuffled deterministically per word id. Tapping a
tile fills the first open blank; filling the last blank checks it; wrong letters turn red
and go back when tapped. "Show the next letter" and "Show the word" are always there.
Solving plays the word. The review's outcome is the worse of the rebuild and the spelling, and its
time is the two parts' time added. After the task, the full card is shown
(picture, definition, example; definition read aloud) with the derived rating (§6), the next
interval, and a four-button control to change the rating. "Say it in your own words" is an
optional note, saved per word and shown on the word card afterwards.

## 3. Transitions

| Step and outcome | Next for that word |
|---|---|
| `know` | `explain` at once |
| `explain` passed | `spell` at once |
| `explain` failed | `explain` again (delayed) |
| `spell` passed | `use` (delayed) |
| `spell` failed | `spell` again (delayed) |
| `use` passed | **graduates**: enters Review |
| `use` failed | `use` again (delayed) |
| `review` rated Again | stays in Review, due the next day; no extra steps today |

A step done again is shuffled differently from the attempt before (its pieces, tiles or
options are seeded with the word id and the attempt number; the first attempt uses the word
id alone).

Graduation rates the word's first FSRS review as **Good**, or **Hard** if any of its
learning steps was shaky or failed. Whatever the rating, its first review is due at the start
of the next local day.

## 4. Pieces

A definition is shown as given text and blanks. Given, never picked:

1. **The word itself**, wherever it appears in any of its forms.
2. **Each sense's opening**: senses are separated by `;`. When a sense names the word within
   its first 12 words, everything up to the word is given, and it runs on
   - through a copula right after the word, with an optional "also" before or after it
     (is / are / means / mean / was / were: "A system is", "a case is also"); or
   - when the sense starts with "If" or "When", to the first `,` `;` `:` within 8 words after
     the word ("If you create something,").

   An opening never takes the whole sense.
3. **Prepositions**: of in on at for from by with about into onto to as over under through
   between among across against during within without behind below above around along towards
   toward upon beneath beyond, and the first word of a two-word preposition (because, instead,
   according, due, apart, such, out before one of them).
4. **Punctuation** that closes a piece (`,` `;` `:` `.` `!` `?`), so a piece's ending never
   shows where it goes.

When nothing would be left to fill, prepositions are blanks after all ("Regarding means
*about*."). Each run of words between given text is cut into blanks:

1. Start a new blank before: that, who, which, where, when, whose, because, but, if, without,
   such, than, while, until, unless (only when the current one has at least 2 words and does not
   end on "or"/"and").
2. End a blank after a word ending in `,` `;` `:` (when it has at least 2 words).
3. A blank longer than MAX words splits nearest its middle, preferring before "or"/"and".
4. One-word scraps merge into the previous blank of the same run.
5. While there are more than MAX_BLANKS blanks, the pair with the fewest words that only
   prepositions (or nothing) separate merges, taking those prepositions in.
6. A definition with a single blank of 4 or more words splits it in two.

MAX is 7 words and MAX_BLANKS 6 for easy puzzles; 4 words and 8 blanks for hard ones. Decoys
are blanks of other words' primary definitions cut the same way, never equal to an answer
piece: 2 decoys (3 when the answer has 5 or more blanks) for easy, 3 (4 when 5 or more) for
hard. Shuffles are deterministic per word id.

## 5. Mixing the stream

The stream is a list of steps chosen one at a time. Parameters: learning window **W = 5**
words, spacing **S = 3** steps, daily new words **N** (setting, default 20), backlog guard
**B = 50** due reviews.

To choose the next step:

0. **Immediate steps.** `explain` after `know` and `spell` after a passed `explain` come
   next, before anything else.
1. **Delayed steps that are ready.** A delayed step is ready when at least S other steps have
   been taken since that word's last step. Take the ready one that has waited longest.
2. **Otherwise alternate review and new.** Take up to two due reviews (lowest FSRS
   retrievability first), then one new word (`know`). A new word is allowed only when fewer
   than W words are Learning, fewer than N words were introduced today, and fewer than B
   reviews are due.
3. **Nothing ready but delayed steps pending.** Take the delayed step that has waited
   longest, even if not yet spaced.
4. **Nothing left.** The stream for today is done.

Two guards apply to every choice when an alternative exists: the same word never appears
in two consecutive steps (except the immediate steps `know` → `explain` → `spell`), and the
same step type never appears more than three times in a row.

Progress shown to the learner: steps done today out of done + remaining, where remaining is
due reviews + pending steps of Learning words + 4 × new words still allowed today.

The learner can pause at any step; the stream resumes exactly where it stopped. "Meet 5 more
words" on the done screen raises today's new-word allowance by 5.

## 6. Ratings

Review ratings are derived from the task, never asked for first:

| Task result | Rating |
|---|---|
| clean and answered within 18 s (rebuild and spelling together) / 4 s (fill-in) | Easy |
| clean | Good |
| shaky | Hard |
| failed | Again |

The derived rating is applied at once and shown with its next interval; tapping another
rating replaces it (the card is rescheduled from its state before this review).

Scheduling is FSRS-6 with the default weights, request retention 0.9, no short-term
(sub-day) steps and no interval fuzz (ts-fsrs `generatorParameters({ enable_short_term:
false, enable_fuzz: false })`); the Android client ports the same long-term scheduler, so
both clients give the same interval. Two dates are fixed rather than taken from FSRS: a
graduated word's first review, and a review rated Again, are both due at the start of the
next local day. Intervals are shown in whole days (at least 1 day).

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
