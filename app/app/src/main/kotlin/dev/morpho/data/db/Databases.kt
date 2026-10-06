package dev.morpho.data.db

import android.content.Context
import android.util.Log
import app.cash.sqldelight.db.QueryResult
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
 * * **release.db** — read-only, shipped in `assets/`. Copied into the databases
 *   directory verbatim on first launch and opened as-is; the generated `Schema` is
 *   never asked to create it.
 * * **user.db** — read/write, created from the generated schema, included in
 *   Android Auto Backup.
 */
class DatabaseProvider(private val context: Context) {

    private val contentDriver: SqlDriver by lazy {
        discardStaleContentDatabase()
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
        discardPreReleaseUserDatabase()
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
     *
     * Runs as a *query*, not a statement: `wal_checkpoint` reports back three columns
     * (busy, log frames, checkpointed frames), and Android's SQLite refuses to execute a
     * row-returning statement through the changed-row-count path. The cursor also has to
     * be stepped, because `rawQuery` does not touch the database until it is read — so
     * an unread cursor would checkpoint nothing at all.
     */
    fun checkpointUserDatabase() {
        runCatching {
            userDriver.executeQuery(
                identifier = null,
                sql = "PRAGMA wal_checkpoint(TRUNCATE);",
                mapper = { cursor -> QueryResult.Value(cursor.next().value) },
                parameters = 0,
            ).value
        }.onFailure { Log.w(TAG, "wal checkpoint failed", it) }
    }

    fun close() {
        runCatching { contentDriver.close() }
        runCatching { userDriver.close() }
    }

    /**
     * Deletes a `user.db` written before the pre-release schema settled.
     *
     * Wave 1 stored `daily_stats.correct_rate`; wave 3 replaced it with exact counts.
     * Neither version ever shipped, so the schema was regenerated outright rather than
     * migrated — which leaves developer devices holding a v1 file whose columns the
     * generated queries no longer name.
     *
     * This is a **one-shot** reset, deliberately pinned to the literal baseline rather
     * than to the current [dev.morpho.domain.model.ProgressDefaults.SCHEMA_VER]: past
     * the first shipped release, user progress is real and a version bump must arrive
     * with an actual SQLDelight migration, not with a deletion.
     */
    private fun discardPreReleaseUserDatabase() {
        val file = context.getDatabasePath(USER_DB_NAME)
        if (!file.isFile) return
        val stored = readUserSchemaVersion(file) ?: return
        if (!isPreReleaseUserSchema(stored)) return

        Log.w(TAG, "user.db schema_ver $stored predates the pre-release baseline; recreating")
        listOf("", "-wal", "-shm").forEach { suffix ->
            File(file.parentFile, file.name + suffix).delete()
        }
    }

    /** Reads `meta.schema_ver` without SQLDelight, whose queries need the new columns. */
    private fun readUserSchemaVersion(file: File): Int? = runCatching {
        android.database.sqlite.SQLiteDatabase.openDatabase(
            file.absolutePath,
            null,
            android.database.sqlite.SQLiteDatabase.OPEN_READONLY,
        ).use { db ->
            db.rawQuery("SELECT value FROM meta WHERE key = 'schema_ver'", null).use { cursor ->
                if (cursor.moveToFirst()) cursor.getString(0)?.trim()?.toIntOrNull() else null
            }
        }
    }.getOrNull()

    /**
     * Throws away a `release.db` copied out of a different build: one whose content DDL
     * was a different shape, or any install before the app was last updated.
     *
     * SQLDelight owns no migrations for this file, and rightly so — it is a derived,
     * read-only artifact that can always be produced again by re-copying the bundled
     * asset. So when the DDL moves (wave 3b adding `gloss_anchors`, say), the answer is
     * to delete the file rather than to migrate it; without this an app updated over an
     * older install opens a database whose columns its generated queries no longer
     * describe, and dies on the first `SELECT *`. The same holds for content: an update
     * that bundles a new release must not keep reading the copy the previous build
     * installed, so the copy is tied to the package's last update time too. User
     * progress lives in user.db, keyed on word ids, and is untouched.
     *
     * The stamp lives in a sibling marker file, not in the database: `user_version`
     * belongs to SQLDelight's open helper, which compares it against its own schema
     * version and would read any other value as an upgrade or a downgrade.
     */
    private fun discardStaleContentDatabase() {
        val database = context.getDatabasePath(CONTENT_DB_NAME)
        val marker = File(database.parentFile, "$CONTENT_DB_NAME$DDL_MARKER_SUFFIX")

        val wanted = "$CONTENT_DDL_VERSION ${packageUpdatedAt()}"
        val stamped = runCatching {
            if (marker.isFile) marker.readText().trim() else null
        }.getOrNull()
        if (stamped == wanted) return

        if (database.exists()) {
            Log.i(TAG, "release.db stamp '$stamped' -> '$wanted'; reinstalling the bundled release")
            listOf("", "-wal", "-shm").forEach { suffix ->
                File(database.parentFile, database.name + suffix).delete()
            }
        }
        runCatching {
            marker.parentFile?.mkdirs()
            marker.writeText(wanted)
        }.onFailure { Log.w(TAG, "could not write the content DDL marker", it) }
    }

    /** When this package was installed or last updated; 0 when the platform will not say. */
    private fun packageUpdatedAt(): Long = runCatching {
        context.packageManager.getPackageInfo(context.packageName, 0).lastUpdateTime
    }.getOrDefault(0L)

    /**
     * Copies the bundled `assets/release.db` into the databases directory the first time
     * it is seen. `noCompress` covers `.db`, so this is a straight byte copy out of the
     * APK rather than an inflate.
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
            stampUserVersion(target)
            Log.i(TAG, "installed bundled release.db (${target.length()} bytes)")
        }.onFailure {
            Log.e(TAG, "failed to install bundled release.db", it)
            target.delete()
        }
    }

    /**
     * Rewrites `PRAGMA user_version` on the installed copy to the version SQLDelight
     * compiled against.
     *
     * This is load-bearing, not defensive. The wave-3 rulings ask the export to set
     * `user_version = schema_ver`, but that number tracks the *content* contract while
     * SQLDelight's open helper compares against its own generated schema version — two
     * counters free to diverge. And the 2026.08.26 release ships `user_version = 0`
     * regardless, which the framework helper reads as "brand new file" and answers by
     * calling `onCreate`, i.e. `CREATE TABLE words` over a table that already has 4253
     * rows in it. Rewriting the stamp before the helper ever opens the file turns both
     * cases into a plain open. Safe to do because this is a private copy the app owns.
     */
    private fun stampUserVersion(file: File) {
        val wanted = ContentDatabase.Schema.version
        runCatching {
            android.database.sqlite.SQLiteDatabase.openDatabase(
                file.absolutePath,
                null,
                android.database.sqlite.SQLiteDatabase.OPEN_READWRITE,
            ).use { db ->
                if (db.version.toLong() != wanted) {
                    Log.i(TAG, "release.db user_version ${db.version} -> $wanted")
                    db.execSQL("PRAGMA user_version = $wanted;")
                }
            }
        }.onFailure { Log.w(TAG, "could not stamp release.db user_version", it) }
    }

    companion object {
        private const val TAG = "DatabaseProvider"
        const val CONTENT_DB_NAME = "release.db"
        const val USER_DB_NAME = "user.db"
        const val BUNDLED_RELEASE_ASSET = "release.db"

        /**
         * Bump whenever the content `.sq` DDL changes shape, so installed copies of
         * `release.db` are rebuilt instead of read with the wrong columns.
         *
         * 1 — wave 1.
         * 2 — wave 3a: `words.etymology_segments`.
         * 3 — wave 3b: `gloss_anchors`, and the first real exported release.
         */
        const val CONTENT_DDL_VERSION = 3

        private const val DDL_MARKER_SUFFIX = ".ddl"

        /** The user.db schema the first release ships. Never raise this — see above. */
        private const val PRE_RELEASE_USER_SCHEMA_VER = 2

        /**
         * True for a `user.db` stamped from before the pre-release baseline settled
         * (wave 1's `daily_stats.correct_rate` shape) — the file
         * [discardPreReleaseUserDatabase] must wipe rather than open. Opening it as-is
         * hands SQLDelight's generated queries a `daily_stats` table that does not name
         * the columns they write (`new_learned`, `reviewed`, ...), so every session's
         * progress write — including the one behind the home screen's "new words today"
         * counter — fails silently against it (2a8e2ed, "schema changes bricking
         * wave-1 installs").
         */
        internal fun isPreReleaseUserSchema(storedSchemaVer: Int): Boolean =
            storedSchemaVer < PRE_RELEASE_USER_SCHEMA_VER
    }
}
