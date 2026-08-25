package dev.morpho.data.seed

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

class EtymologyTextTest {

    @Test
    fun `segments round-trip through the encoded string`() {
        val encoded = EtymologyText.encode(
            listOf("bene", "vol", "ent"),
            "From Latin bene, meaning well.",
        )
        val parsed = EtymologyText.parse(encoded)
        assertEquals(listOf("bene", "vol", "ent"), parsed.segments)
        assertEquals("From Latin bene, meaning well.", parsed.prose)
    }

    @Test
    fun `plain prose from a real export renders as prose with no chips`() {
        val parsed = EtymologyText.parse("From Old French, meaning to hold fast in a storm.")
        assertTrue(parsed.segments.isEmpty())
        assertEquals("From Old French, meaning to hold fast in a storm.", parsed.prose)
    }

    @Test
    fun `prose containing an em dash is not mistaken for segments`() {
        val raw = "Borrowed from Latin — the sense of kindness came later."
        val parsed = EtymologyText.parse(raw)
        assertTrue(parsed.segments.isEmpty())
        assertEquals(raw, parsed.prose)
    }

    @Test
    fun `segments without prose still parse`() {
        val parsed = EtymologyText.parse(EtymologyText.encode(listOf("meta", "morph", "osis"), null))
        assertEquals(listOf("meta", "morph", "osis"), parsed.segments)
        assertEquals(null, parsed.prose)
    }

    @Test
    fun `null and blank input yield nothing`() {
        assertTrue(EtymologyText.parse(null).segments.isEmpty())
        assertEquals(null, EtymologyText.parse(null).prose)
        assertTrue(EtymologyText.parse("   ").segments.isEmpty())
    }

    @Test
    fun `byte highlight handles multi-byte characters ahead of the target`() {
        val sentence = "Café — a benevolent host waved us in."
        val (start, end) = DemoSeedWriter.byteHighlight(sentence, "benevolent")
        val bytes = sentence.toByteArray(Charsets.UTF_8)
        assertEquals("benevolent", String(bytes, start, end - start, Charsets.UTF_8))
    }

    @Test
    fun `media names are stable and differ per key`() {
        val a = DemoSeedWriter.mediaName("img", "benevolent-image", "png")
        val b = DemoSeedWriter.mediaName("img", "benevolent-image", "png")
        val c = DemoSeedWriter.mediaName("img", "malevolent-image", "png")
        assertEquals(a, b)
        assertTrue(a != c)
        assertTrue(a.startsWith("img/") && a.endsWith(".png"))
    }
}
