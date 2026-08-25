package dev.morpho.domain.review

import dev.morpho.domain.model.FsrsCard
import dev.morpho.domain.util.SplitMix64
import dev.morpho.domain.util.seedOf
import java.time.Instant

/** The two review question types (README Part 1, "复习"). */
enum class ReviewQuestionType {
    /** Show the primary definition, pick the word out of four. */
    DEFINITION_TO_WORD,

    /** Play the word audio, spell it letter by letter. */
    LISTENING_SPELL,
}

data class ReviewItem(
    val card: FsrsCard,
    val type: ReviewQuestionType,
)

/**
 * Decides *what* to review today and *how* to ask it.
 *
 * Interval maths lives in [FsrsScheduler]; this object only handles queue building
 * and the lapse-weighted question-type choice: a word the user keeps forgetting is
 * increasingly asked as a listening-spell (production recall) rather than a
 * four-way definition match (recognition), because recognition stops discriminating
 * once the word is familiar-but-not-known.
 */
class ReviewScheduler(
    private val fsrs: FsrsScheduler = FsrsScheduler(),
    private val weights: TypeWeights = TypeWeights(),
) {

    data class TypeWeights(
        /** Probability of a listening-spell for a card that has never lapsed. */
        val baseSpellProbability: Double = 0.20,
        /** Added per lapse. */
        val spellProbabilityPerLapse: Double = 0.20,
        /** Ceiling so definition-to-word never disappears entirely. */
        val maxSpellProbability: Double = 0.80,
    )

    /** Cards whose `due` has arrived, earliest first. */
    fun dueCards(cards: List<FsrsCard>, now: Instant): List<FsrsCard> =
        cards.filter { !it.due.isAfter(now) }.sortedBy { it.due }

    fun spellProbability(lapses: Int): Double =
        (weights.baseSpellProbability + weights.spellProbabilityPerLapse * lapses)
            .coerceIn(0.0, weights.maxSpellProbability)

    /**
     * Picks the question type for [card]. Deterministic per (word, day) via [daySeed]
     * so re-entering the screen does not reshuffle the question in front of the user.
     */
    fun questionTypeFor(card: FsrsCard, daySeed: Long): ReviewQuestionType {
        val roll = SplitMix64(seedOf(daySeed, card.wordId)).nextInt(1_000_000) / 1_000_000.0
        return if (roll < spellProbability(card.lapses)) {
            ReviewQuestionType.LISTENING_SPELL
        } else {
            ReviewQuestionType.DEFINITION_TO_WORD
        }
    }

    /** Full review queue for the day, capped at [limit] when set. */
    fun buildQueue(
        cards: List<FsrsCard>,
        now: Instant,
        daySeed: Long,
        limit: Int? = null,
    ): List<ReviewItem> {
        val due = dueCards(cards, now)
        val capped = if (limit != null) due.take(limit) else due
        return capped.map { ReviewItem(it, questionTypeFor(it, daySeed)) }
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
