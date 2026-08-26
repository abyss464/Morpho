package dev.morpho.data.backup

import app.cash.sqldelight.driver.jdbc.sqlite.JdbcSqliteDriver
import dev.morpho.data.db.user.UserDatabase
import dev.morpho.domain.model.ProgressDefaults
import java.io.ByteArrayInputStream
import java.io.File
import java.sql.DriverManager
import kotlin.test.AfterTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertIs
import kotlin.test.assertTrue

/**
 * Import replaces everything the user has, so these are the tests that keep a mistake
 * from being unrecoverable. Real SQLite files built from the real generated schema, not
 * hand-waved fixtures: the header check, the table check and the version check each have
 * to hold against a file the app itself would have written.
 */
class ProgressBackupValidatorTest {

    private val temporaries = mutableListOf<File>()

    @AfterTest
    fun cleanUp() {
        temporaries.forEach { it.delete() }
    }

    private fun tempFile(name: String): File =
        File.createTempFile(name, ".db").also { temporaries += it }

    /** Builds a real user.db on disk, exactly as the app's schema generates it. */
    private fun userDatabase(schemaVer: String? = ProgressDefaults.SCHEMA_VER.toString()): File {
        val file = tempFile("user")
        file.delete() // JdbcSqliteDriver wants to create it
        val driver = JdbcSqliteDriver("jdbc:sqlite:${file.absolutePath}")
        UserDatabase.Schema.create(driver)
        val db = UserDatabase(driver)
        if (schemaVer != null) db.userMetaQueries.upsert("schema_ver", schemaVer)
        db.learningProgressQueries.upsert(1L, 2L, 3L, "learning")
        driver.close()
        return file
    }

    private fun verdictFor(file: File, schemaVerOverride: String? = null): BackupVerdict {
        val tables = tablesIn(file)
        val schemaVer = schemaVerOverride ?: metaValue(file, "schema_ver")
        return ProgressBackupValidator.validate(
            file = file,
            tables = tables,
            schemaVerRaw = schemaVer,
            supportedSchemaVer = ProgressDefaults.SCHEMA_VER,
        )
    }

    private fun tablesIn(file: File): Set<String> = runCatching {
        DriverManager.getConnection("jdbc:sqlite:${file.absolutePath}").use { conn ->
            conn.createStatement().use { st ->
                st.executeQuery("SELECT name FROM sqlite_master WHERE type = 'table'").use { rs ->
                    buildSet { while (rs.next()) add(rs.getString(1)) }
                }
            }
        }
    }.getOrDefault(emptySet())

    private fun metaValue(file: File, key: String): String? = runCatching {
        DriverManager.getConnection("jdbc:sqlite:${file.absolutePath}").use { conn ->
            conn.prepareStatement("SELECT value FROM meta WHERE key = ?").use { st ->
                st.setString(1, key)
                st.executeQuery().use { rs -> if (rs.next()) rs.getString(1) else null }
            }
        }
    }.getOrNull()

    // ------------------------------------------------------------- the header

    @Test
    fun `a real user database passes the header sniff`() {
        assertTrue(ProgressBackupValidator.looksLikeSqlite(userDatabase()))
    }

    @Test
    fun `a photo the user picked by mistake is not a database`() {
        val jpeg = tempFile("holiday")
        jpeg.writeBytes(byteArrayOf(0xFF.toByte(), 0xD8.toByte(), 0xFF.toByte()) + ByteArray(4096))
        assertFalse(ProgressBackupValidator.looksLikeSqlite(jpeg))
        assertIs<BackupVerdict.NotSqlite>(verdictFor(jpeg))
    }

    @Test
    fun `an empty file is not a database`() {
        val empty = tempFile("empty")
        empty.writeBytes(ByteArray(0))
        assertFalse(ProgressBackupValidator.looksLikeSqlite(empty))
    }

    @Test
    fun `a file shorter than the magic is not a database`() {
        val runt = tempFile("runt")
        runt.writeBytes("SQLite".toByteArray())
        assertFalse(ProgressBackupValidator.looksLikeSqlite(runt))
    }

    @Test
    fun `the magic must match to the trailing NUL`() {
        // "SQLite format 3" followed by a space instead of NUL: close is not a match.
        val impostor = "SQLite format 3 ".toByteArray(Charsets.US_ASCII) + ByteArray(64)
        assertFalse(ProgressBackupValidator.looksLikeSqlite(ByteArrayInputStream(impostor)))
        assertTrue(
            ProgressBackupValidator.looksLikeSqlite(
                ByteArrayInputStream(ProgressBackupValidator.SQLITE_MAGIC + ByteArray(64)),
            ),
        )
    }

    @Test
    fun `a stream delivering the header one byte at a time still matches`() {
        val dribbling = object : java.io.InputStream() {
            private val bytes = ProgressBackupValidator.SQLITE_MAGIC
            private var index = 0
            override fun read(): Int = if (index < bytes.size) bytes[index++].toInt() and 0xFF else -1
            override fun read(b: ByteArray, off: Int, len: Int): Int {
                if (index >= bytes.size) return -1
                b[off] = bytes[index++]
                return 1
            }
        }
        assertTrue(ProgressBackupValidator.looksLikeSqlite(dribbling))
    }

    // ------------------------------------------------------------- the tables

    @Test
    fun `a database from another app is refused by name`() {
        val other = tempFile("someone-else")
        other.delete()
        val driver = JdbcSqliteDriver("jdbc:sqlite:${other.absolutePath}")
        driver.execute(null, "CREATE TABLE notes (id INTEGER PRIMARY KEY, body TEXT);", 0)
        driver.close()

        val verdict = verdictFor(other)
        val missing = assertIs<BackupVerdict.MissingTables>(verdict).missing
        assertEquals(ProgressBackupValidator.REQUIRED_TABLES.sorted(), missing)
    }

    @Test
    fun `a release database is not a progress backup`() {
        // The realistic wrong pick: the user has both files and grabs the content one.
        val release = tempFile("release")
        release.delete()
        val driver = JdbcSqliteDriver("jdbc:sqlite:${release.absolutePath}")
        driver.execute(null, "CREATE TABLE words (word_id INTEGER PRIMARY KEY);", 0)
        driver.execute(null, "CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);", 0)
        driver.execute(null, "INSERT INTO meta VALUES ('schema_ver', '1');", 0)
        driver.close()

        val missing = assertIs<BackupVerdict.MissingTables>(verdictFor(release)).missing
        assertEquals(listOf("daily_stats", "fsrs_cards", "learning_progress"), missing)
    }

    // ------------------------------------------------------------ the version

    @Test
    fun `a backup this build wrote is accepted`() {
        val verdict = verdictFor(userDatabase())
        assertEquals(ProgressDefaults.SCHEMA_VER, assertIs<BackupVerdict.Ok>(verdict).schemaVer)
        assertTrue(verdict.isOk)
    }

    @Test
    fun `an older schema is still readable`() {
        val verdict = verdictFor(userDatabase(), schemaVerOverride = "1")
        assertEquals(1, assertIs<BackupVerdict.Ok>(verdict).schemaVer)
    }

    @Test
    fun `a newer schema is refused with both numbers named`() {
        val newer = ProgressDefaults.SCHEMA_VER + 1
        val verdict = verdictFor(userDatabase(), schemaVerOverride = newer.toString())
        val rejected = assertIs<BackupVerdict.NewerSchema>(verdict)
        assertEquals(newer, rejected.found)
        assertEquals(ProgressDefaults.SCHEMA_VER, rejected.supported)
        assertFalse(verdict.isOk)
    }

    @Test
    fun `a missing schema version is refused rather than assumed`() {
        assertIs<BackupVerdict.MissingSchemaVersion>(verdictFor(userDatabase(schemaVer = null)))
        assertIs<BackupVerdict.MissingSchemaVersion>(
            verdictFor(userDatabase(), schemaVerOverride = "   "),
        )
    }

    @Test
    fun `a garbled schema version is refused rather than coerced`() {
        val verdict = verdictFor(userDatabase(), schemaVerOverride = "2.0-beta")
        assertEquals("2.0-beta", assertIs<BackupVerdict.UnreadableSchemaVersion>(verdict).raw)
    }

    @Test
    fun `surrounding whitespace on the version is tolerated`() {
        val verdict = verdictFor(userDatabase(), schemaVerOverride = " 2 ")
        assertEquals(2, assertIs<BackupVerdict.Ok>(verdict).schemaVer)
    }

    // --------------------------------------------------------------- ordering

    @Test
    fun `the header is checked before anything else is trusted`() {
        // A text file whose "tables" and version are supplied by a lying caller must
        // still be refused: the bytes decide first.
        val text = tempFile("notes")
        text.writeText("learning_progress fsrs_cards daily_stats meta schema_ver=2")
        val verdict = ProgressBackupValidator.validate(
            file = text,
            tables = ProgressBackupValidator.REQUIRED_TABLES,
            schemaVerRaw = "2",
            supportedSchemaVer = ProgressDefaults.SCHEMA_VER,
        )
        assertIs<BackupVerdict.NotSqlite>(verdict)
    }

    @Test
    fun `a directory is never a backup`() {
        val dir = File.createTempFile("dir", "").also { temporaries += it }
        dir.delete()
        dir.mkdirs()
        assertFalse(ProgressBackupValidator.looksLikeSqlite(dir))
        dir.deleteRecursively()
    }
}
