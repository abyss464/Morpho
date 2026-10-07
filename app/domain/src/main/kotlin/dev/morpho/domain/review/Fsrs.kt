package dev.morpho.domain.review

import dev.morpho.domain.model.CardState
import dev.morpho.domain.model.FsrsCard
import java.math.BigDecimal
import java.math.RoundingMode
import java.time.Duration
import java.time.Instant
import java.time.ZoneOffset
import java.time.temporal.ChronoUnit

/**
 * FSRS-6 with the long-term scheduler: a port of ts-fsrs 5.4.2's `FSRSAlgorithm` and
 * `LongTermScheduler` as the web client runs them, `generatorParameters({ enable_short_term:
 * false, enable_fuzz: false })` (docs/contracts/stream.md §6). For the same card, grade and
 * time both clients give the same card.
 *
 * What the port keeps from ts-fsrs, so the numbers agree to the last digit:
 *  * every intermediate value is rounded as `roundTo(x, 8)` with JavaScript's `Math.round`;
 *  * `exp`, `pow` and `log` go through [StrictMath] (fdlibm), as V8's do;
 *  * days since the last review are UTC calendar days for a review, and whole 24 h periods
 *    for retrievability;
 *  * with short-term off there are no learning steps and no same-day formula: every review
 *    takes the long-term path, every card is in [CardState.REVIEW] afterwards, and a lapse
 *    never raises stability (the Again floor `S / e^(w17 * w18)` is `S / e^0`).
 *
 * ## Formulas (w = the 21 weights, G = grade 1..4)
 *
 * ```
 * DECAY  = -w20,  FACTOR = 0.9^(1/DECAY) - 1
 * R(t, S)  = (1 + FACTOR * t / S)^DECAY
 * I(S)     = S * (0.9^(1/DECAY) - 1) / FACTOR, rounded, in [1, 36500]
 * S0(G)    = max(w[G-1], 0.1)
 * D0(G)    = w4 - e^((G-1) * w5) + 1
 * D'       = clamp(w7 * D0(4) + (1 - w7) * (D + (-w6 * (G-3)) * (10 - D) / 9), 1, 10)
 * S'r      = S * (1 + e^w8 * (11 - D) * S^-w9 * (e^((1-R) * w10) - 1) * (w15 if Hard) * (w16 if Easy))
 * S'f      = min(S, w11 * D^-w12 * ((S+1)^w13 - 1) * e^((1-R) * w14))
 * ```
 *
 * The four intervals of one review are kept apart: Again <= Hard < Good < Easy.
 */
object Fsrs {

    /** FSRS-6 default weights w0..w20 (ts-fsrs `default_w`). */
    val DEFAULT_PARAMETERS: DoubleArray = doubleArrayOf(
        0.212, 1.2931, 2.3065, 8.2956, 6.4133, 0.8334, 3.0194, 0.001,
        1.8722, 0.1666, 0.796, 1.4835, 0.0614, 0.2629, 1.6483, 0.6014,
        1.8729, 0.5425, 0.0912, 0.0658, 0.1542,
    )

    const val MIN_STABILITY: Double = 0.001
    const val MAX_STABILITY: Double = 36500.0
    const val MAX_INTERVAL_DAYS: Long = 36500
    const val DEFAULT_RETENTION: Double = 0.9
}

/** The four FSRS grades. */
enum class Grade(val value: Int) {
    AGAIN(1),
    HARD(2),
    GOOD(3),
    EASY(4),
}

/**
 * Stateless FSRS-6 long-term scheduler.
 *
 * @param parameters the 21 weights, used as given (the defaults lie within ts-fsrs's clamps)
 * @param requestRetention target recall probability at review time
 * @param maximumInterval the longest interval in days
 */
class FsrsScheduler(
    private val parameters: DoubleArray = Fsrs.DEFAULT_PARAMETERS,
    private val requestRetention: Double = Fsrs.DEFAULT_RETENTION,
    private val maximumInterval: Long = Fsrs.MAX_INTERVAL_DAYS,
) {
    init {
        require(parameters.size == 21) { "FSRS-6 needs 21 parameters, got ${parameters.size}" }
        require(requestRetention > 0.0 && requestRetention <= 1.0) { "requestRetention must be in (0, 1]" }
    }

    private fun w(i: Int) = parameters[i]

    private val decay: Double = -w(20)
    private val factor: Double = roundTo(StrictMath.exp(StrictMath.pow(decay, -1.0) * StrictMath.log(0.9)) - 1, 8)
    private val intervalModifier: Double =
        roundTo((StrictMath.pow(requestRetention, 1 / decay) - 1) / factor, 8)

    // ------------------------------------------------------------ core formulas

    /** `R(t, S)`, rounded to 8 decimals. */
    fun forgettingCurve(elapsedDays: Double, stability: Double): Double =
        roundTo(StrictMath.pow(1 + factor * elapsedDays / stability, decay), 8)

    /** Retrievability of [card] at [now], from whole days since its last review; 0 for a new card. */
    fun retrievability(card: FsrsCard, now: Instant): Double {
        val last = card.lastReview
        if (card.state == CardState.NEW || last == null) return 0.0
        val t = maxOf(Math.floorDiv(Duration.between(last, now).toMillis(), DAY_MS), 0L)
        return forgettingCurve(t.toDouble(), toFixed8(card.stability))
    }

    private fun initStability(g: Int): Double = maxOf(w(g - 1), 0.1)

    private fun initDifficulty(g: Int): Double = roundTo(w(4) - StrictMath.exp((g - 1) * w(5)) + 1, 8)

    private fun nextInterval(stability: Double): Long =
        minOf(maxOf(1.0, jsRound(stability * intervalModifier)), maximumInterval.toDouble()).toLong()

    private fun linearDamping(deltaD: Double, oldD: Double): Double = roundTo(deltaD * (10 - oldD) / 9, 8)

    private fun meanReversion(init: Double, current: Double): Double = roundTo(w(7) * init + (1 - w(7)) * current, 8)

    private fun nextDifficulty(d: Double, g: Int): Double {
        val deltaD = -w(6) * (g - 3)
        val nextD = d + linearDamping(deltaD, d)
        return clamp(meanReversion(initDifficulty(Grade.EASY.value), nextD), 1.0, 10.0)
    }

    private fun nextRecallStability(d: Double, s: Double, r: Double, g: Int): Double {
        val hardPenalty = if (g == Grade.HARD.value) w(15) else 1.0
        val easyBound = if (g == Grade.EASY.value) w(16) else 1.0
        return roundTo(
            clamp(
                s * (1 + StrictMath.exp(w(8)) * (11 - d) * StrictMath.pow(s, -w(9)) *
                    (StrictMath.exp((1 - r) * w(10)) - 1) * hardPenalty * easyBound),
                Fsrs.MIN_STABILITY,
                Fsrs.MAX_STABILITY,
            ),
            8,
        )
    }

    private fun nextForgetStability(d: Double, s: Double, r: Double): Double = roundTo(
        clamp(
            w(11) * StrictMath.pow(d, -w(12)) * (StrictMath.pow(s + 1, w(13)) - 1) *
                StrictMath.exp((1 - r) * w(14)),
            Fsrs.MIN_STABILITY,
            Fsrs.MAX_STABILITY,
        ),
        8,
    )

    /** Difficulty and stability after grade [g], [t] days after the last review. */
    private fun nextState(d: Double, s: Double, t: Long, g: Int, retrievability: Double?): Pair<Double, Double> {
        if (d == 0.0 && s == 0.0) return clamp(initDifficulty(g), 1.0, 10.0) to initStability(g)
        require(d >= 1 && s >= Fsrs.MIN_STABILITY) { "Invalid memory state { difficulty: $d, stability: $s }" }
        val r = retrievability ?: forgettingCurve(t.toDouble(), s)
        val newS = if (g == Grade.AGAIN.value) {
            // Short-term off: w17 and w18 count as 0, so the floor is S itself.
            val afterFail = nextForgetStability(d, s, r)
            clamp(roundTo(s / StrictMath.exp(0.0), 8), Fsrs.MIN_STABILITY, afterFail)
        } else {
            nextRecallStability(d, s, r, g)
        }
        return nextDifficulty(d, g) to newS
    }

    // ------------------------------------------------------------------ review

    /** The card after each of the four grades, reviewed at [now] (ts-fsrs `repeat`). */
    fun preview(card: FsrsCard, now: Instant): Map<Grade, FsrsCard> {
        val last = card.lastReview
        val isNew = card.state == CardState.NEW
        val elapsed = if (!isNew && last != null) utcDaysBetween(last, now) else 0L
        // A new card starts from t = 0 with no retrievability; a reviewed one from its R at t.
        val r = if (isNew) null else forgettingCurve(elapsed.toDouble(), card.stability)
        val t = if (isNew) 0L else elapsed
        val states = Grade.entries.associateWith { g -> nextState(card.difficulty, card.stability, t, g.value, r) }

        val again = nextInterval(states.getValue(Grade.AGAIN).second)
        val hard0 = nextInterval(states.getValue(Grade.HARD).second)
        val good0 = nextInterval(states.getValue(Grade.GOOD).second)
        val easy0 = nextInterval(states.getValue(Grade.EASY).second)
        val againDays = minOf(again, hard0)
        val hardDays = maxOf(hard0, againDays + 1)
        val goodDays = maxOf(good0, hardDays + 1)
        val easyDays = maxOf(easy0, goodDays + 1)
        val days = mapOf(Grade.AGAIN to againDays, Grade.HARD to hardDays, Grade.GOOD to goodDays, Grade.EASY to easyDays)

        return Grade.entries.associateWith { g ->
            val (difficulty, stability) = states.getValue(g)
            val interval = days.getValue(g)
            card.copy(
                due = now.plusMillis(interval * DAY_MS),
                stability = stability,
                difficulty = difficulty,
                elapsedDays = t.toInt(),
                scheduledDays = interval.toInt(),
                reps = card.reps + 1,
                lapses = card.lapses + if (g == Grade.AGAIN && !isNew) 1 else 0,
                state = CardState.REVIEW,
                lastReview = now,
            )
        }
    }

    /** Applies a review of [card] at [now] with [grade] and returns the rescheduled card. */
    fun review(card: FsrsCard, grade: Grade, now: Instant): FsrsCard = preview(card, now).getValue(grade)

    /** The first card of a word that has just graduated, reviewed once with [grade]. */
    fun newCard(wordId: Long, now: Instant, grade: Grade = Grade.GOOD): FsrsCard {
        val blank = FsrsCard(wordId = wordId, due = now, stability = 0.0, difficulty = 0.0, state = CardState.NEW)
        return review(blank, grade, now)
    }

    private companion object {
        const val DAY_MS = 86_400_000L

        /** JavaScript's `Math.round`: the nearest integer, halves towards +infinity. */
        fun jsRound(x: Double): Double {
            val f = Math.floor(x)
            return if (x - f >= 0.5) f + 1 else f
        }

        /** ts-fsrs `roundTo`. */
        fun roundTo(x: Double, decimals: Int): Double {
            val factor = StrictMath.pow(10.0, decimals.toDouble())
            return jsRound(x * factor) / factor
        }

        fun clamp(value: Double, min: Double, max: Double): Double = minOf(maxOf(value, min), max)

        /** `+x.toFixed(8)`: exact decimal rounding of the binary value, then parsed back. */
        fun toFixed8(x: Double): Double = BigDecimal(x).setScale(8, RoundingMode.HALF_UP).toDouble()

        /** ts-fsrs `dateDiffInDays`: the difference of the two UTC calendar dates. */
        fun utcDaysBetween(last: Instant, now: Instant): Long =
            ChronoUnit.DAYS.between(last.atOffset(ZoneOffset.UTC).toLocalDate(), now.atOffset(ZoneOffset.UTC).toLocalDate())
    }
}
