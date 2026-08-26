package dev.morpho.data.content

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull
import kotlin.test.assertTrue

/**
 * `words.etymology_segments` is the only structured source for the chips, so the parser
 * has to be both faithful to a well-formed export and unshakable on a bad one: a single
 * malformed row must cost that word its chips, never the whole detail sheet.
 */
class EtymologySegmentsTest {

    @Test
    fun `a contract-shaped array becomes chips in order`() {
        assertEquals(
            listOf("bene", "vol", "ent"),
            EtymologySegments.parse("""["bene","vol","ent"]"""),
        )
    }

    @Test
    fun `a single-segment array is still an array`() {
        assertEquals(listOf("morph"), EtymologySegments.parse("""["morph"]"""))
    }

    @Test
    fun `null and blank mean prose-only, not an error`() {
        assertTrue(EtymologySegments.parse(null).isEmpty())
        assertTrue(EtymologySegments.parse("").isEmpty())
        assertTrue(EtymologySegments.parse("   ").isEmpty())
        assertTrue(EtymologySegments.parse("[]").isEmpty())
    }

    @Test
    fun `prose accidentally left in the column degrades to no chips`() {
        // The wave-1 shim would have chipped this. The column is JSON or it is nothing.
        assertTrue(EtymologySegments.parse("bene + vol + ent").isEmpty())
        assertTrue(EtymologySegments.parse("From Latin bene, meaning well.").isEmpty())
    }

    @Test
    fun `malformed json degrades instead of throwing`() {
        assertTrue(EtymologySegments.parse("""["bene","vol""").isEmpty())
        assertTrue(EtymologySegments.parse("{}").isEmpty())
        assertTrue(EtymologySegments.parse("""{"segments":["a"]}""").isEmpty())
    }

    @Test
    fun `non-string and empty entries are dropped, the rest survive`() {
        assertEquals(
            listOf("bene", "vol"),
            EtymologySegments.parse("""["bene", 7, null, "  ", "vol"]"""),
        )
    }

    @Test
    fun `segments are trimmed`() {
        assertEquals(listOf("bene", "vol"), EtymologySegments.parse("""["  bene", "vol  "]"""))
    }

    @Test
    fun `unicode segments round-trip through encode`() {
        val segments = listOf("prōd", "esse")
        val encoded = EtymologySegments.encode(segments)
        assertEquals(segments, EtymologySegments.parse(encoded))
    }

    @Test
    fun `encoding nothing yields a null column rather than an empty array`() {
        assertNull(EtymologySegments.encode(emptyList()))
        assertNull(EtymologySegments.encode(listOf("", "   ")))
    }
}
