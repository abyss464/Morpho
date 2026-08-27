package dev.morpho.data.db

import kotlin.test.Test
import kotlin.test.assertFalse
import kotlin.test.assertTrue

/**
 * Pins the wave-1 -> wave-3a `user.db` discard boundary (backlog B3, "home screen
 * daily counter does not update after a learning session").
 *
 * Root cause, confirmed against project history: wave 1 (6ff2ac2) shipped
 * `daily_stats.correct_rate`, a stored rate. Wave 3a (2a8e2ed) replaced it with exact
 * counts (`new_learned`, `reviewed`, `correct_count`, `answer_count`) and bumped
 * `schema_ver` 1 -> 2, but the DDL was regenerated outright rather than migrated —
 * SQLDelight's own `PRAGMA user_version` never moves, since there are no `.sqm`
 * migration files, so its open helper cannot tell the shape changed. A device holding
 * a wave-1 `user.db` that upgrades straight to the new schema would have every
 * `daily_stats` write (the write [dev.morpho.ui.learn.LearnViewModel.finish] makes,
 * that the home screen's "new words today" counter reads back) fail against columns
 * the old table does not have — reproducing exactly "I finished a lesson and the
 * counter is still 0/50."
 *
 * [DatabaseProvider.discardPreReleaseUserDatabase] guards against this by wiping a
 * `user.db` stamped below the pre-release baseline before the driver ever opens it.
 * This test pins the boundary that guard is built on, since nothing previously
 * exercised it on the JVM.
 */
class DatabaseProviderTest {

    @Test
    fun `a wave-1 user_db predates the baseline and must be discarded`() {
        assertTrue(DatabaseProvider.isPreReleaseUserSchema(1))
    }

    @Test
    fun `the wave-3a baseline and anything shipped since is kept`() {
        assertFalse(DatabaseProvider.isPreReleaseUserSchema(2))
        assertFalse(DatabaseProvider.isPreReleaseUserSchema(3))
    }
}
