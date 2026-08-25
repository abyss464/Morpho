package dev.morpho.ui.designsystem.component

/**
 * Visual state of a single quiz option, shared by the three grids.
 *
 * Motion per docs/contracts/app-design.md:
 *  * [PRESSED] scale 1 -> 0.97, 100 ms
 *  * [CORRECT] ring + check badge spring 250 ms, other cells fade to 0.4
 *  * [WRONG]   horizontal shake +/-8dp spring 300 ms, cell dims
 */
enum class QuizOptionState {
    IDLE,
    PRESSED,
    CORRECT,
    WRONG,

    /** Not chosen, but the answer has been revealed — fade back. */
    DIMMED,

    /** The correct cell, revealed after the user got it wrong. */
    REVEALED,
}

/** Alpha applied to unselected cells once an answer has been revealed. */
const val DIMMED_OPTION_ALPHA = 0.4f
