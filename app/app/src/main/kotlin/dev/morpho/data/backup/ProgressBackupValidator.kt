package dev.morpho.data.backup

import java.io.File
import java.io.InputStream

/**
 * Decides whether a user-picked file is a progress snapshot this build can restore.
 *
 * Import replaces everything the user has, so the check runs *before* anything is
 * swapped and errs towards refusing. Three questions, in order of how cheap they are:
 *
 *  1. Is it a SQLite database at all? (16-byte magic — catches a picked photo instantly)
 *  2. Does it carry our four tables? (catches a `release.db`, or some other app's DB)
 *  3. Is `meta.schema_ver` one this build understands? (a newer file is refused rather
 *     than silently half-read — the columns it gained are not knowable here)
 *
 * Everything here is a pure function over bytes and already-extracted metadata, so it is
 * unit-tested directly with no device, driver, or Android runtime.
 */
object ProgressBackupValidator {

    /** The first 16 bytes of every SQLite 3 file, including the trailing NUL. */
    val SQLITE_MAGIC: ByteArray = "SQLite format 3".toByteArray(Charsets.US_ASCII) + 0

    val REQUIRED_TABLES: Set<String> = setOf(
        "learning_progress",
        "fsrs_cards",
        "daily_stats",
        "meta",
    )

    /** Reads only the header. Does not close [source]. */
    fun looksLikeSqlite(source: InputStream): Boolean {
        val header = ByteArray(SQLITE_MAGIC.size)
        var read = 0
        while (read < header.size) {
            val n = source.read(header, read, header.size - read)
            if (n < 0) return false
            read += n
        }
        return header.contentEquals(SQLITE_MAGIC)
    }

    fun looksLikeSqlite(file: File): Boolean =
        file.isFile && file.length() >= SQLITE_MAGIC.size &&
            file.inputStream().use { looksLikeSqlite(it) }

    /**
     * @param tables table names read from the candidate's `sqlite_master`
     * @param schemaVerRaw the candidate's `meta.schema_ver` value, verbatim
     * @param supportedSchemaVer what this build writes ([dev.morpho.domain.model.ProgressDefaults.SCHEMA_VER])
     */
    fun validate(
        file: File,
        tables: Set<String>,
        schemaVerRaw: String?,
        supportedSchemaVer: Int,
    ): BackupVerdict {
        if (!looksLikeSqlite(file)) return BackupVerdict.NotSqlite

        val missing = REQUIRED_TABLES.filterNot { it in tables }.sorted()
        if (missing.isNotEmpty()) return BackupVerdict.MissingTables(missing)

        val raw = schemaVerRaw?.trim()
        if (raw.isNullOrEmpty()) return BackupVerdict.MissingSchemaVersion
        val found = raw.toIntOrNull() ?: return BackupVerdict.UnreadableSchemaVersion(raw)
        if (found > supportedSchemaVer) {
            return BackupVerdict.NewerSchema(found = found, supported = supportedSchemaVer)
        }
        return BackupVerdict.Ok(schemaVer = found)
    }
}

/** Outcome of [ProgressBackupValidator.validate]. Only [Ok] may be applied. */
sealed interface BackupVerdict {

    data class Ok(val schemaVer: Int) : BackupVerdict

    data object NotSqlite : BackupVerdict

    data class MissingTables(val missing: List<String>) : BackupVerdict

    data object MissingSchemaVersion : BackupVerdict

    data class UnreadableSchemaVersion(val raw: String) : BackupVerdict

    /** Written by a newer build; its extra columns cannot be interpreted here. */
    data class NewerSchema(val found: Int, val supported: Int) : BackupVerdict

    val isOk: Boolean get() = this is Ok
}
