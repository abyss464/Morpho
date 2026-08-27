package dev.morpho.domain.review

import dev.morpho.domain.model.FsrsCard
import java.time.Instant

/**
 * One slot in the review queue. Formerly carried a question type; since wave 1 every
 * review uses the unified image+definition grid (mode-2 visual), so the type field is
 * gone and the card is the only payload.
 */
data class ReviewItem(
    val card: FsrsCard,
)

/**
 * Decides *what* to review today.
 *
 * Interval maths lives in [FsrsScheduler]; this object handles queue building.
 * Since wave 1 every review uses the unified image+definition grid, so the
 * lapse-weighted question-type branching is gone.
 */
class ReviewScheduler(
    private val fsrs: FsrsScheduler = FsrsScheduler(),
) {

    /** Cards whose `due` has arrived, earliest first. */
    fun dueCards(cards: List<FsrsCard>, now: Instant): List<FsrsCard> =
        cards.filter { !it.due.isAfter(now) }.sortedBy { it.due }

    /** Full review queue for the day, capped at [limit] when set. */
    fun buildQueue(
        cards: List<FsrsCard>,
        now: Instant,
        limit: Int? = null,
    ): List<ReviewItem> {
        val due = dueCards(cards, now)
        val capped = if (limit != null) due.take(limit) else due
        return capped.map { ReviewItem(it) }
    }

    /**
     * Grades a review. Morpho asks binary questions, so the mapping is:
     * correct on the first attempt -> [Grade.GOOD], anything else -> [Grade.AGAIN].
     */
    fun grade(correct: Boolean): Grade = if (correct) Grade.GOOD else Grade.AGAIN

    fun applyAnswer(card: FsrsCard, correct: Boolean, now: Instant): FsrsCard =
        fsrs.review(card, grade(correct), now)

    fun newCardFor(wordId: Long, now: Instant): FsrsCard = fsrs.newCard(wordId, now)

    fun scheduler(): FsrsScheduler = fsrs
}
