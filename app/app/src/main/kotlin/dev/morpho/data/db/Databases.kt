package dev.morpho.data.db

import android.content.Context
import android.util.Log
import app.cash.sqldelight.db.SqlDriver
import app.cash.sqldelight.driver.android.AndroidSqliteDriver
import dev.morpho.data.db.content.ContentDatabase
import dev.morpho.data.db.user.UserDatabase
import java.io.File

/**
 * Opens the app's two SQLDelight databases.
 *
 * Both use SQLDelight rather than Room, deliberately: `release.db` is produced by an
 * external tool, so Room's schema-ownership checks and migration machinery are pure
 * friction (README Part 6). SQLDelight compiles typed queries against the exported
 * DDL and demands nothing of the file at runtime.
 *
 * * **release.db** — read-only. If `assets/release.db` exists it is copied into place
 *   verbatim on first launch and opened as-is. Otherwise (wave 1, no core exporter yet)
 *   the generated schema creates an empty DB that `DemoContentSeeder` fills.
 * * **user.db** — read/write, created from the generated schema, included in
 *   Android Auto Backup.
 */
class DatabaseProvider(private val context: Context) {

    private val contentDriver: SqlDriver by lazy {
        installBundledReleaseIfPresent()
        AndroidSqliteDriver(
            schema = ContentDatabase.Schema,
            context = context,
            name = CONTENT_DB_NAME,
            callback = object : AndroidSqliteDriver.Callback(ContentDatabase.Schema) {
                override fun onOpen(db: androidx.sqlite.db.SupportSQLiteDatabase) {
                    db.execSQL("PRAGMA foreign_keys = ON;")
                }
            },
        )
    }

    private val userDriver: SqlDriver by lazy {
        AndroidSqliteDriver(
            schema = UserDatabase.Schema,
            context = context,
            name = USER_DB_NAME,
            callback = object : AndroidSqliteDriver.Callback(UserDatabase.Schema) {
                override fun onOpen(db: androidx.sqlite.db.SupportSQLiteDatabase) {
                    db.execSQL("PRAGMA foreign_keys = ON;")
                }
            },
        )
    }

    val contentDatabase: ContentDatabase by lazy { ContentDatabase(contentDriver) }
    val userDatabase: UserDatabase by lazy { UserDatabase(userDriver) }

    /** Absolute path of user.db, for the export/import flow. */
    fun userDatabaseFile(): File = context.getDatabasePath(USER_DB_NAME)

    fun contentDatabaseFile(): File = context.getDatabasePath(CONTENT_DB_NAME)

    /**
     * Flushes the WAL so a copy of `user.db` is complete on its own.
     * Required before backup or manual export (README Part 6).
     */
    fun checkpointUserDatabase() {
        runCatching {
            userDriver.execute(null, "PRAGMA wal_checkpoint(TRUNCATE);", 0)
        }.onFailure { Log.w(TAG, "wal checkpoint failed", it) }
    }

    fun close() {
        runCatching { contentDriver.close() }
        runCatching { userDriver.close() }
    }

    /**
     * Copies a bundled `assets/release.db` into the databases directory the first
     * time it is seen. This is the wave-2 path; wave 1 simply has no such asset.
     */
    private fun installBundledReleaseIfPresent() {
        val target = context.getDatabasePath(CONTENT_DB_NAME)
        if (target.exists()) return
        val available = runCatching {
            context.assets.open(BUNDLED_RELEASE_ASSET).use { true }
        }.getOrDefault(false)
        if (!available) return

        target.parentFile?.mkdirs()
        runCatching {
            context.assets.open(BUNDLED_RELEASE_ASSET).use { input ->
                target.outputStream().use { output -> input.copyTo(output) }
            }
            Log.i(TAG, "installed bundled release.db (${target.length()} bytes)")
        }.onFailure {
            Log.e(TAG, "failed to install bundled release.db", it)
            target.delete()
        }
    }

    companion object {
        private const val TAG = "DatabaseProvider"
        const val CONTENT_DB_NAME = "release.db"
        const val USER_DB_NAME = "user.db"
        const val BUNDLED_RELEASE_ASSET = "release.db"
    }
}
