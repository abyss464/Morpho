package dev.morpho.domain.progress

/**
 * How much of a running session has already been written into `daily_stats`.
 *
 * A learning session is a full daily batch — 50 new words over three rounds, roughly 150
 * answers. Storing its totals only at the summary screen meant a session abandoned part
 * way stored nothing at all, so the home counter sat at `0 / goal` however many words the
 * user had actually finished (backlog #35). Worse, the next start sizes its quota from
 * that same row, so a day that never banked kept handing out an untouched full batch.
 *
 * Progress is therefore banked as it happens: each word that clears its third round moves
 * the counter immediately. [pending] is what makes repeating that safe — it reports only
 * the part of the running totals this watermark has not seen, so banking after every
 * answer and again at the end counts each word exactly once.
 */
data class SessionBank(
    val learned: Int = 0,
    val correctAnswers: Int = 0,
    val totalAnswers: Int = 0,
) {

    /** True when there is nothing left to write. */
    val isEmpty: Boolean
        get() = learned == 0 && correctAnswers == 0 && totalAnswers == 0

    /**
     * The slice of [running] this watermark has not banked yet. Advance the watermark to
     * [running] once the write succeeds; calling again with unchanged totals then yields
     * an [isEmpty] result rather than a duplicate.
     */
    fun pending(running: SessionBank): SessionBank = SessionBank(
        learned = running.learned - learned,
        correctAnswers = running.correctAnswers - correctAnswers,
        totalAnswers = running.totalAnswers - totalAnswers,
    )
}
