package dev.morpho.domain.review

import dev.morpho.domain.model.CardState
import dev.morpho.domain.model.FsrsCard
import java.time.Duration
import java.time.Instant
import kotlin.math.exp
import kotlin.math.ln
import kotlin.math.max
import kotlin.math.min
import kotlin.math.pow
import kotlin.math.roundToLong

/**
 * FSRS v5 (Free Spaced Repetition Scheduler), implemented from the published algorithm.
 *
 * Reference: open-spaced-repetition, "The Algorithm" (FSRS-5). All formulas below are
 * written out in the comments so the implementation can be audited without leaving the file.
 *
 * ## Memory state
 *
 * A card carries two latent variables:
 *  * **S** (stability) — days until retrievability decays to 90%.
 *  * **D** (difficulty) — intrinsic hardness in `[1, 10]`.
 *
 * ## Forgetting curve (power law, FSRS-4.5+)
 *
 * ```
 * DECAY  = -0.5
 * FACTOR = 0.9^(1/DECAY) - 1 = 19/81
 * R(t, S) = (1 + FACTOR * t / S)^DECAY
 * ```
 *
 * ## Interval for a desired retention r
 *
 * ```
 * I(r, S) = (S / FACTOR) * (r^(1/DECAY) - 1)
 * ```
 *
 * ## First review (grade G in 1..4)
 *
 * ```
 * S_0(G) = w[G-1]                              , clamped to >= 0.1
 * D_0(G) = w[4] - e^(w[5] * (G - 1)) + 1       , clamped to [1, 10]
 * ```
 *
 * ## Difficulty update (linear damping + mean reversion, new in FSRS-5)
 *
 * ```
 * dD    = -w[6] * (G - 3)
 * D'    = D + dD * (10 - D) / 9                 // linear damping
 * D''   = w[7] * D_0(4) + (1 - w[7]) * D'       // mean reversion towards "Easy" difficulty
 * ```
 *
 * ## Stability on successful recall (G in 2..4)
 *
 * ```
 * S'_r = S * (1 + e^(w[8]) * (11 - D) * S^(-w[9]) * (e^((1 - R) * w[10]) - 1)
 *              * hardPenalty * easyBonus)
 * hardPenalty = w[15] if G == 2 else 1
 * easyBonus   = w[16] if G == 4 else 1
 * ```
 *
 * ## Stability on lapse (G == 1)
 *
 * ```
 * S'_f = min(
 *          w[11] * D^(-w[12]) * ((S + 1)^w[13] - 1) * e^((1 - R) * w[14]),
 *          S / e^(w[17] * w[18])                   // FSRS-5 cap: a lapse never raises S
 *        )
 * ```
 *
 * ## Same-day re-review (FSRS-5 short-term stability)
 *
 * ```
 * S'_s = S * e^(w[17] * (G - 3 + w[18]))
 * ```
 *
 * ## Deviation from the reference scheduler
 *
 * The reference implementation keeps sub-day learning/relearning *steps* on the card.
 * `docs/contracts/user-db.sql` has no `step` column, and Morpho only reviews at
 * day granularity (a word becomes a card only after it has cleared the three-round
 * learning ladder). This scheduler therefore runs with empty learning/relearning
 * steps, which is a supported configuration of the reference algorithm: a card goes
 * straight to [CardState.REVIEW] and a lapse re-enters [CardState.RELEARNING] with a
 * day-based interval. Interval fuzzing is likewise off, so scheduling is deterministic
 * and testable.
 */
object Fsrs {

    const val DECAY: Double = -0.5

    /** `0.9^(1/DECAY) - 1`, i.e. 19/81. */
    val FACTOR: Double = 0.9.pow(1.0 / DECAY) - 1.0

    /** FSRS-5 default weights w0..w18. */
    val DEFAULT_PARAMETERS: DoubleArray = doubleArrayOf(
        0.40255, 1.18385, 3.173, 15.69105, 7.1949, 0.5345, 1.4604, 0.0046,
        1.54575, 0.1192, 1.01925, 1.9395, 0.11, 0.29605, 2.2698, 0.2315,
        2.9898, 0.51655, 0.6621,
    )

    const val MIN_STABILITY: Double = 0.1
    const val MIN_DIFFICULTY: Double = 1.0
    const val MAX_DIFFICULTY: Double = 10.0
    const val MAX_INTERVAL_DAYS: Long = 36500
}

/** The four FSRS grades. Morpho maps a binary answer onto AGAIN / GOOD. */
enum class Grade(val value: Int) {
    AGAIN(1),
    HARD(2),
    GOOD(3),
    EASY(4),
}

/**
 * Stateless FSRS v5 scheduler.
 *
 * @param parameters the 19 model weights
 * @param desiredRetention target recall probability at review time (0.9 by default)
 */
class FsrsScheduler(
    private val parameters: DoubleArray = Fsrs.DEFAULT_PARAMETERS,
    private val desiredRetention: Double = 0.9,
    private val maximumIntervalDays: Long = Fsrs.MAX_INTERVAL_DAYS,
) {
    init {
        require(parameters.size == 19) { "FSRS v5 needs 19 parameters, got ${parameters.size}" }
        require(desiredRetention > 0.0 && desiredRetention < 1.0) {
            "desiredRetention must be in (0, 1)"
        }
    }

    private fun w(i: Int) = parameters[i]

    // ------------------------------------------------------------ core formulas

    /** `R(t, S) = (1 + FACTOR * t / S)^DECAY` */
    fun retrievability(elapsedDays: Double, stability: Double): Double {
        if (stability <= 0.0) return 0.0
        val t = max(0.0, elapsedDays)
        return (1.0 + Fsrs.FACTOR * t / stability).pow(Fsrs.DECAY)
    }

    /** Retrievability of [card] as of [now]; 0 for a card that has never been reviewed. */
    fun retrievability(card: FsrsCard, now: Instant): Double {
        val last = card.lastReview ?: return 0.0
        val elapsed = max(0L, Duration.between(last, now).toDays())
        return retrievability(elapsed.toDouble(), card.stability)
    }

    /** `I(r, S) = (S / FACTOR) * (r^(1/DECAY) - 1)`, rounded to whole days, min 1. */
    fun intervalDays(stability: Double): Long {
        val raw = (stability / Fsrs.FACTOR) * (desiredRetention.pow(1.0 / Fsrs.DECAY) - 1.0)
        return raw.roundToLong().coerceIn(1L, maximumIntervalDays)
    }

    /** `S_0(G) = w[G-1]`, floored at 0.1. */
    fun initialStability(grade: Grade): Double =
        max(w(grade.value - 1), Fsrs.MIN_STABILITY)

    /** `D_0(G) = w[4] - e^(w[5] * (G - 1)) + 1`, clamped to [1, 10]. */
    fun initialDifficulty(grade: Grade): Double =
        (w(4) - exp(w(5) * (grade.value - 1)) + 1.0)
            .coerceIn(Fsrs.MIN_DIFFICULTY, Fsrs.MAX_DIFFICULTY)

    /** Linear damping followed by mean reversion towards `D_0(EASY)`. */
    fun nextDifficulty(difficulty: Double, grade: Grade): Double {
        val delta = -(w(6) * (grade.value - 3))
        val damped = difficulty + (10.0 - difficulty) * delta / 9.0
        val reverted = w(7) * initialDifficulty(Grade.EASY) + (1.0 - w(7)) * damped
        return reverted.coerceIn(Fsrs.MIN_DIFFICULTY, Fsrs.MAX_DIFFICULTY)
    }

    fun nextRecallStability(
        difficulty: Double,
        stability: Double,
        retrievability: Double,
        grade: Grade,
    ): Double {
        val hardPenalty = if (grade == Grade.HARD) w(15) else 1.0
        val easyBonus = if (grade == Grade.EASY) w(16) else 1.0
        return stability * (
            1.0 + exp(w(8)) *
                (11.0 - difficulty) *
                stability.pow(-w(9)) *
                (exp((1.0 - retrievability) * w(10)) - 1.0) *
                hardPenalty *
                easyBonus
            )
    }

    fun nextForgetStability(
        difficulty: Double,
        stability: Double,
        retrievability: Double,
    ): Double {
        val longTerm = w(11) *
            difficulty.pow(-w(12)) *
            ((stability + 1.0).pow(w(13)) - 1.0) *
            exp((1.0 - retrievability) * w(14))
        val shortTermCap = stability / exp(w(17) * w(18))
        return min(longTerm, shortTermCap)
    }

    /** `S'_s = S * e^(w17 * (G - 3 + w18))` — same-day re-review. */
    fun shortTermStability(stability: Double, grade: Grade): Double =
        stability * exp(w(17) * (grade.value - 3 + w(18)))

    fun nextStability(
        difficulty: Double,
        stability: Double,
        retrievability: Double,
        grade: Grade,
    ): Double = when (grade) {
        Grade.AGAIN -> nextForgetStability(difficulty, stability, retrievability)
        else -> nextRecallStability(difficulty, stability, retrievability, grade)
    }

    // ------------------------------------------------------------------ review

    /**
     * Applies a review of [card] at [now] with [grade] and returns the rescheduled card.
     *
     * A brand-new card ([CardState.NEW], or one with no prior review) is initialised.
     * Same-day repeats use the short-term stability formula and do not re-lapse.
     */
    fun review(card: FsrsCard, grade: Grade, now: Instant): FsrsCard {
        val daysSinceLastReview = card.lastReview
            ?.let { Duration.between(it, now).toDays() }

        val isFirstReview = card.state == CardState.NEW || card.lastReview == null
        val isSameDayRepeat = !isFirstReview && (daysSinceLastReview ?: 0L) < 1L

        val stability: Double
        val difficulty: Double
        when {
            isFirstReview -> {
                stability = initialStability(grade)
                difficulty = initialDifficulty(grade)
            }

            isSameDayRepeat -> {
                stability = shortTermStability(card.stability, grade)
                difficulty = nextDifficulty(card.difficulty, grade)
            }

            else -> {
                val r = retrievability(card, now)
                stability = nextStability(card.difficulty, card.stability, r, grade)
                difficulty = nextDifficulty(card.difficulty, grade)
            }
        }

        val boundedStability = max(stability, Fsrs.MIN_STABILITY)
        val lapsed = grade == Grade.AGAIN && !isFirstReview
        val nextState = when {
            lapsed -> CardState.RELEARNING
            else -> CardState.REVIEW
        }
        val interval = intervalDays(boundedStability)

        return card.copy(
            due = now.plus(Duration.ofDays(interval)),
            stability = boundedStability,
            difficulty = difficulty,
            elapsedDays = (daysSinceLastReview ?: 0L).coerceAtLeast(0L).toInt(),
            scheduledDays = interval.toInt(),
            reps = card.reps + 1,
            lapses = card.lapses + if (grade == Grade.AGAIN) 1 else 0,
            state = nextState,
            lastReview = now,
        )
    }

    /** Creates the first card for a word that has just cleared the learning ladder. */
    fun newCard(wordId: Long, now: Instant, grade: Grade = Grade.GOOD): FsrsCard {
        val blank = FsrsCard(
            wordId = wordId,
            due = now,
            stability = 0.0,
            difficulty = 0.0,
            state = CardState.NEW,
        )
        return review(blank, grade, now)
    }

    /**
     * Number of days until [card] decays to [target] retrievability, from its last review.
     * Handy for the "next review in N days" copy on the summary screen.
     */
    fun daysUntilRetention(card: FsrsCard, target: Double = desiredRetention): Long {
        if (card.stability <= 0.0) return 0
        val exponent = ln(target) / Fsrs.DECAY
        return ((exp(exponent) - 1.0) * card.stability / Fsrs.FACTOR).roundToLong()
    }
}
