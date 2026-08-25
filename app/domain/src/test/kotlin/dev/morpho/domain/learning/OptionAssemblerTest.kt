package dev.morpho.domain.learning

import dev.morpho.domain.util.deterministicShuffled
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertTrue

class OptionAssemblerTest {

    @Test
    fun `assembles the answer plus its three bound distractors`() {
        val options = OptionAssembler.assemble(10L, listOf(11L, 12L, 13L), seed = 7L)
        assertEquals(4, options.size)
        assertEquals(setOf(10L, 11L, 12L, 13L), options.toSet())
        assertEquals(options.indexOf(10L), OptionAssembler.answerIndex(options, 10L))
    }

    @Test
    fun `the same seed always produces the same order`() {
        val a = OptionAssembler.assemble(10L, listOf(11L, 12L, 13L), seed = 99L)
        val b = OptionAssembler.assemble(10L, listOf(11L, 12L, 13L), seed = 99L)
        assertEquals(a, b)
    }

    @Test
    fun `different seeds reach every answer position`() {
        val positions = (0L until 200L)
            .map { OptionAssembler.assemble(10L, listOf(11L, 12L, 13L), it).indexOf(10L) }
            .toSet()
        assertEquals(setOf(0, 1, 2, 3), positions)
    }

    @Test
    fun `malformed distractor sets are rejected loudly`() {
        assertFailsWith<IllegalArgumentException> {
            OptionAssembler.assemble(10L, listOf(11L, 12L), seed = 1L)
        }
        assertFailsWith<IllegalArgumentException> {
            OptionAssembler.assemble(10L, listOf(10L, 11L, 12L), seed = 1L)
        }
    }

    @Test
    fun `deterministic shuffle is a permutation and is stable`() {
        val input = (1L..40L).toList()
        val once = input.deterministicShuffled(12345L)
        val twice = input.deterministicShuffled(12345L)
        assertEquals(once, twice)
        assertEquals(input.toSet(), once.toSet())
        assertTrue(once != input, "shuffle produced the identity permutation")
    }
}
