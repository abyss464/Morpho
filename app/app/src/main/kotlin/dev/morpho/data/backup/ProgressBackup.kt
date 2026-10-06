package dev.morpho.data.backup

import android.content.Context
import android.content.Intent
import android.database.sqlite.SQLiteDatabase
import android.net.Uri
import android.util.Log
import dev.morpho.data.db.DatabaseProvider
import dev.morpho.domain.model.ProgressDefaults
import dev.morpho.domain.model.UserMetaKeys
import java.io.File
import java.time.LocalDate
import java.time.format.DateTimeFormatter
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/**
 * Manual progress backup over the Storage Access Framework.
 *
 * Export writes `user.db` verbatim to a document the user picks, after a WAL checkpoint
 * so the single file is complete on its own. Import is the mirror image, plus a refusal
 * path: the picked file is staged next to the live database, validated
 * ([ProgressBackupValidator]), confirmed by the user, then swapped in with a rename —
 * atomic because both paths are in the same directory — and the process is restarted so
 * nothing keeps a handle on the file that just disappeared.
 *
 * No `ACTION_GET_CONTENT`, no storage permission: `ACTION_CREATE_DOCUMENT` and
 * `ACTION_OPEN_DOCUMENT` hand back a URI the user chose, which is all this needs.
 */
class ProgressBackup(
    private val context: Context,
    private val databaseProvider: DatabaseProvider,
) {

    /** `morpho-progress-20260826.db` — sorts chronologically in any file browser. */
    fun suggestedFileName(today: LocalDate = LocalDate.now()): String =
        "$FILE_PREFIX${today.format(DateTimeFormatter.BASIC_ISO_DATE)}$FILE_SUFFIX"

    // ---------------------------------------------------------------- export

    /** @return bytes written, or the failure that stopped it. */
    suspend fun export(target: Uri): Result<Long> = withContext(Dispatchers.IO) {
        runCatching {
            // Without this the newest rows may still be sitting in the WAL and the
            // copy would silently restore to an older state.
            databaseProvider.checkpointUserDatabase()
            val source = databaseProvider.userDatabaseFile()
            check(source.isFile) { "user.db does not exist yet" }

            val written = context.contentResolver.openOutputStream(target, "wt")
                ?.use { output -> source.inputStream().use { it.copyTo(output) } }
                ?: error("could not open $target for writing")
            Log.i(TAG, "exported $written bytes to $target")
            written
        }.onFailure { Log.e(TAG, "export failed", it) }
    }

    // ---------------------------------------------------------------- import

    /**
     * Copies the picked document next to the live database and inspects it. Nothing is
     * replaced yet — the caller shows [StagedBackup.verdict] and asks for confirmation.
     */
    suspend fun stage(source: Uri): StagedBackup = withContext(Dispatchers.IO) {
        val staged = stagingFile()
        staged.delete()

        val copied = runCatching {
            context.contentResolver.openInputStream(source)
                ?.use { input -> staged.outputStream().use { input.copyTo(it) } }
                ?: error("could not open $source for reading")
        }.onFailure { Log.e(TAG, "could not stage $source", it) }.isSuccess

        if (!copied) {
            staged.delete()
            return@withContext StagedBackup(null, BackupVerdict.NotSqlite, null)
        }

        val probe = probe(staged)
        val verdict = ProgressBackupValidator.validate(
            file = staged,
            tables = probe.tables,
            schemaVerRaw = probe.schemaVer,
            supportedSchemaVer = ProgressDefaults.SCHEMA_VER,
        )
        if (!verdict.isOk) {
            staged.delete()
            return@withContext StagedBackup(null, verdict, probe.summary)
        }
        StagedBackup(staged, verdict, probe.summary)
    }

    /** Drops a staged file the user backed out of. */
    suspend fun discard(staged: StagedBackup) = withContext(Dispatchers.IO) {
        staged.file?.delete()
        Unit
    }

    /**
     * Replaces `user.db` with the staged snapshot and restarts into it.
     *
     * The swap has to close the drivers first — SQLite must not hold a handle on a file
     * that is about to be renamed out from under it — and closing them leaves the
     * repositories above holding dead references, so once this method starts swapping it
     * always ends in a restart, successful or not. That is why the only `false` it
     * returns is the pre-flight refusal below, where nothing has been touched yet and
     * the screen can still say something useful.
     *
     * The outgoing database is moved aside rather than deleted, so the window in which
     * the user has no progress is a single rename wide and a failed swap rolls straight
     * back. The WAL and SHM side files belong to the database being replaced and would
     * otherwise be replayed over the new one, so they go first.
     *
     * @return false if the staged file was refused; otherwise does not return.
     */
    suspend fun applyAndRestart(staged: StagedBackup): Boolean {
        val file = staged.file ?: return false
        if (!staged.verdict.isOk || !file.isFile) return false

        withContext(Dispatchers.IO) {
            databaseProvider.close()
            val target = databaseProvider.userDatabaseFile()
            val displaced = File(target.parentFile, target.name + REPLACED_SUFFIX)

            runCatching {
                listOf(WAL_SUFFIX, SHM_SUFFIX).forEach { suffix ->
                    File(target.parentFile, target.name + suffix).delete()
                }
                displaced.delete()
                if (target.exists()) {
                    check(target.renameTo(displaced)) { "could not move the live database aside" }
                }
                if (!file.renameTo(target)) {
                    // Nothing lost yet: put the original back before giving up.
                    displaced.renameTo(target)
                    error("could not move the snapshot into place")
                }
                Log.i(TAG, "imported progress snapshot (${target.length()} bytes)")
            }.onFailure { Log.e(TAG, "import swap failed, progress left unchanged", it) }

            displaced.delete()
            file.delete()
        }

        restartProcess()
        return true
    }

    /**
     * Relaunches into a fresh process.
     *
     * A restart rather than a re-open: view models, the settings StateFlow, the FSRS
     * scheduler and the session holder all cache state read from the database that was
     * just replaced, and reconciling them one by one is far more fragile than starting
     * clean.
     */
    fun restartProcess() {
        val launch = context.packageManager.getLaunchIntentForPackage(context.packageName)
        if (launch == null) {
            Log.e(TAG, "no launch intent; cannot restart")
            return
        }
        launch.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TASK)
        context.startActivity(launch)
        Runtime.getRuntime().exit(0)
    }

    // ------------------------------------------------------------- internals

    private fun stagingFile(): File {
        val target = databaseProvider.userDatabaseFile()
        target.parentFile?.mkdirs()
        // Same directory as the live database on purpose: File.renameTo is only
        // guaranteed atomic within one filesystem, and the cache dir is not one.
        return File(target.parentFile, target.name + IMPORT_SUFFIX)
    }

    /** Reads what the validator needs out of a candidate file, tolerating garbage. */
    private fun probe(file: File): Probe {
        if (!ProgressBackupValidator.looksLikeSqlite(file)) return Probe(emptySet(), null, null)
        return runCatching {
            SQLiteDatabase.openDatabase(
                file.absolutePath,
                null,
                SQLiteDatabase.OPEN_READONLY,
            ).use { db ->
                val tables = mutableSetOf<String>()
                db.rawQuery("SELECT name FROM sqlite_master WHERE type = 'table'", null)
                    .use { c -> while (c.moveToNext()) tables += c.getString(0) }

                val schemaVer = if ("meta" in tables) {
                    db.rawQuery(
                        "SELECT value FROM meta WHERE key = ?",
                        arrayOf(UserMetaKeys.SCHEMA_VER),
                    ).use { c -> if (c.moveToFirst()) c.getString(0) else null }
                } else {
                    null
                }

                val summary = if (ProgressBackupValidator.REQUIRED_TABLES.all { it in tables }) {
                    BackupSummary(
                        cardsScheduled = db.count("SELECT count(*) FROM fsrs_cards"),
                        daysRecorded = db.count("SELECT count(*) FROM daily_stats"),
                    )
                } else {
                    null
                }
                Probe(tables, schemaVer, summary)
            }
        }.onFailure { Log.w(TAG, "cannot probe candidate backup", it) }
            .getOrDefault(Probe(emptySet(), null, null))
    }

    private fun SQLiteDatabase.count(sql: String): Int =
        rawQuery(sql, null).use { c -> if (c.moveToFirst()) c.getInt(0) else 0 }

    private data class Probe(
        val tables: Set<String>,
        val schemaVer: String?,
        val summary: BackupSummary?,
    )

    companion object {
        private const val TAG = "ProgressBackup"

        const val FILE_PREFIX = "morpho-progress-"
        const val FILE_SUFFIX = ".db"

        /** SAF has no registered type for SQLite; a generic binary type keeps pickers open. */
        const val MIME_TYPE = "application/octet-stream"

        private const val IMPORT_SUFFIX = ".import"
        private const val REPLACED_SUFFIX = ".replaced"
        private const val WAL_SUFFIX = "-wal"
        private const val SHM_SUFFIX = "-shm"
    }
}

/** A candidate sitting next to the live database, waiting on the user's confirmation. */
data class StagedBackup(
    val file: File?,
    val verdict: BackupVerdict,
    val summary: BackupSummary?,
)

/** What the confirm dialog tells the user they are about to overwrite their progress with. */
data class BackupSummary(
    /** Words in review: one FSRS card each. */
    val cardsScheduled: Int,
    val daysRecorded: Int,
)
