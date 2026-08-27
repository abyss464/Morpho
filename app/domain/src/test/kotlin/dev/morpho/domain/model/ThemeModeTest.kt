package dev.morpho.domain.model

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

/**
 * The theme preference round-trips through one `meta` string and resolves against the
 * platform's day/night state. Both halves matter: a value that fails to round-trip
 * silently resets the user's choice on the next launch, and a resolution that ignores
 * the platform makes "System" a third fixed scheme.
 */
class ThemeModeTest {

    @Test
    fun `every mode round-trips through its stored value`() {
        ThemeMode.entries.forEach { mode ->
            assertEquals(mode, ThemeMode.fromDb(mode.dbValue), "round trip for $mode")
        }
    }

    @Test
    fun `stored values are the lowercase names the meta table holds`() {
        assertEquals("system", ThemeMode.SYSTEM.dbValue)
        assertEquals("light", ThemeMode.LIGHT.dbValue)
        assertEquals("dark", ThemeMode.DARK.dbValue)
    }

    @Test
    fun `an unknown or cleared value falls back to following the system`() {
        assertEquals(ThemeMode.SYSTEM, ThemeMode.fromDb(""))
        assertEquals(ThemeMode.SYSTEM, ThemeMode.fromDb("sepia"))
        // Case matters: only what dbValue writes is recognised.
        assertEquals(ThemeMode.SYSTEM, ThemeMode.fromDb("DARK"))
    }

    @Test
    fun `system follows the platform, both ways`() {
        assertTrue(ThemeMode.SYSTEM.isDark(systemInDark = true))
        assertFalse(ThemeMode.SYSTEM.isDark(systemInDark = false))
    }

    @Test
    fun `an explicit choice overrides the platform, both ways`() {
        assertFalse(ThemeMode.LIGHT.isDark(systemInDark = true))
        assertFalse(ThemeMode.LIGHT.isDark(systemInDark = false))
        assertTrue(ThemeMode.DARK.isDark(systemInDark = true))
        assertTrue(ThemeMode.DARK.isDark(systemInDark = false))
    }
}
