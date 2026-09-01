---
file: app/app/src/main/kotlin/dev/morpho/ui/review/ReviewViewModel.kt
---

# Review Session ViewModel

Drives a single review session. All questions uniformly use an image + definition grid (dual visual mode). FSRS determines which cards are due; each answer directly updates the card's scheduling parameters. Creating one automatically starts the session.

## Exposed State (ReviewUiState)

- **loading** — currently loading
- **finished** — session has ended; UI should navigate to the summary page
- **empty** — no due review cards
- **question** — current question (see ReviewQuestionUi), null when there are no questions
- **index / total** — current question number / total number of questions
- **selectedIndex** — index of the user's selected option
- **revealed** — answer already revealed
- **mustRetry** — after a wrong answer, must select the correct one to continue
- **feedback** — current feedback signal (correct/error/none)
- **detail** — word detail page data; when non-empty, a detail overlay should appear
- **nowPlayingFile** — the audio file name currently being played

## ReviewQuestionUi

Display data for a review question: word ID, word text, pronunciation, word audio, list of image options, index of the correct option, and all senses of the word.

## Accepted Actions

### onOptionSelected(index)
User clicks an option. On a wrong answer: play error sound and haptics; the first wrong answer immediately writes a failed score to the FSRS card and enters must-retry status. On a correct answer: play the correct sound; the first correct answer writes a success score. After a wrong answer, selecting the correct one pops up the detail overlay; a directly correct answer automatically advances.

### onDetailDismissed()
Closes the detail overlay and advances to the next question.

### onPlayAudio(file)
Manually plays the specified audio file.

### onReplayWord()
Replays the pronunciation of the current question's word.

### onExit()
Exits midway, stopping the audio.
