package dev.morpho.domain.content

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull
import kotlin.test.assertTrue

class GlossIndexTest {

    private val index = GlossIndex.of(
        listOf(
            GlossAnchor(1, "rites", "仪式"),
            GlossAnchor(2, "pertaining", "与…有关"),
            GlossAnchor(3, "upper", "上面的"),
            GlossAnchor(4, "mother-in-law", "岳母"),
        ),
    )

    @Test
    fun `an anchored token is found with its gloss`() {
        val matches = index.scan("the upper deck")
        assertEquals(1, matches.size)
        assertEquals(4, matches[0].start)
        assertEquals(9, matches[0].endExclusive)
        assertEquals("upper", matches[0].lemma)
        assertEquals("上面的", matches[0].gloss)
    }

    @Test
    fun `matching ignores case but reports the release's own spelling`() {
        val matches = index.scan("Rites of passage.")
        assertEquals(1, matches.size)
        assertEquals(0 until 5, matches[0].start until matches[0].endExclusive)
        assertEquals("rites", matches[0].lemma)
    }

    @Test
    fun `an anchor never fires inside a longer word`() {
        assertTrue(index.scan("favourites").isEmpty())
        assertTrue(index.scan("uppercase").isEmpty())
        assertTrue(index.scan("appertaining").isEmpty())
    }

    @Test
    fun `punctuation and quotes do not hide an anchor`() {
        assertEquals(1, index.scan("(rites)").size)
        assertEquals(1, index.scan("“upper”").size)
        assertEquals(1, index.scan("rites, and more").size)
    }

    @Test
    fun `a hyphenated lemma stays one token`() {
        val matches = index.scan("her mother-in-law arrived")
        assertEquals(1, matches.size)
        assertEquals("mother-in-law", matches[0].lemma)
    }

    @Test
    fun `every anchor in a sentence is reported in reading order and none overlap`() {
        val matches = index.scan("rites pertaining to the upper hall")
        assertEquals(listOf("rites", "pertaining", "upper"), matches.map { it.lemma })
        matches.zipWithNext { a, b -> assertTrue(a.endExclusive <= b.start) }
    }

    @Test
    fun `text with no anchors allocates no matches`() {
        assertTrue(index.scan("a quiet afternoon").isEmpty())
        assertTrue(GlossIndex.EMPTY.scan("rites").isEmpty())
        assertTrue(GlossIndex.EMPTY.isEmpty)
    }

    @Test
    fun `single-token lookup mirrors the scan`() {
        assertEquals("仪式", index.gloss("RITES"))
        assertNull(index.gloss("favourites"))
    }

    @Test
    fun `a duplicate lemma resolves deterministically to the lowest word id`() {
        val duplicated = GlossIndex.of(
            listOf(GlossAnchor(9, "Upper", "第二个"), GlossAnchor(3, "upper", "上面的")),
        )
        assertEquals(1, duplicated.size)
        assertEquals("上面的", duplicated.gloss("upper"))
    }

    @Test
    fun `a match reports which character offsets it covers`() {
        val match = index.scan("the upper deck").single()
        assertTrue(4 in match)
        assertTrue(8 in match)
        assertTrue(9 !in match)
        assertTrue(3 !in match)
    }
}
