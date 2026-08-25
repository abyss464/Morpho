package dev.morpho.domain.learning

import dev.morpho.domain.util.deterministicShuffled

/**
 * Builds the four-way option set for every quiz and review question.
 *
 * Product rule (README Part 1, "干扰项"): a word's three distractors are bound once at
 * content-build time and never change. The engine therefore never picks options — it
 * only decides the *order*, and that order is a pure function of the question's
 * identity so a retry after a wrong answer shows the same layout the user just looked at.
 */
object OptionAssembler {

    const val OPTION_COUNT = 4

    /**
     * @param answerWordId the word being tested
     * @param distractorIds exactly three bound distractors, in rank order
     * @param seed from [Question.optionSeed]
     * @return the four word ids in presentation order
     */
    fun assemble(answerWordId: Long, distractorIds: List<Long>, seed: Long): List<Long> {
        require(distractorIds.size == 3) {
            "word $answerWordId must have exactly 3 bound distractors, got ${distractorIds.size}"
        }
        require(answerWordId !in distractorIds) {
            "word $answerWordId cannot be its own distractor"
        }
        return (listOf(answerWordId) + distractorIds).deterministicShuffled(seed)
    }

    /** Index of the correct cell within [assemble]'s result. */
    fun answerIndex(options: List<Long>, answerWordId: Long): Int =
        options.indexOf(answerWordId).also {
            require(it >= 0) { "answer $answerWordId missing from assembled options" }
        }
}
