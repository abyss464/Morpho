---
file: app/app/src/main/kotlin/dev/morpho/ui/home/ActionCard.kt
---

# rows/lines Dynamic Card

The only card on the home page that the user is required to use, serving as the entry point for learning and review.

## ActionCard(today, hasContent, onStartLearning, onStartReview, modifier)

- today: Today's progress data (new words learned, daily goal, reviewed count, pending review count, correct count, total answer count)
- hasContent: Whether the word database has content to learn. When false, the card only shows an empty-state message.
- onStartLearning: Triggered when the "Start Learning" button is clicked
- onStartReview: Triggered when the "Start Review" button is clicked

### Card Content

1. **Two metric blocks** displayed side by side:
   - Pending review count — highlighted when there are items to review, dimmed when it is 0
   - New learning progress — displayed as "learned / goal"
2. **Progress rows/lines** — uses icon-based progress markers to show the daily goal completion ratio
3. **Status text** — shows a congratulatory message when everything is complete
4. **Buttons** — layout determined by priority:
   - When there are items to review: the review button is the primary button (solid), and the learning button is the secondary button (outlined)
   - When there are no items to review: the learning button becomes the primary button; if all of today's tasks are complete, it is disabled
