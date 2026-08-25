package dev.morpho.data.seed

import android.content.Context
import android.util.Log
import dev.morpho.data.db.content.ContentDatabase
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.Json

/**
 * Fills an empty release.db from `assets/demo/demo_content.json`.
 *
 * Wave 1 has no `morphod export` yet, so the app creates release.db from the
 * SQLDelight schema (which mirrors `docs/contracts/release-db.sql` exactly) and seeds
 * it with invented but schema-valid content. The demo therefore runs against a real
 * database behind real queries — only the rows are fake.
 *
 * When a real release.db lands in `assets/`, `DatabaseProvider` installs it verbatim
 * and this seeder finds a non-empty database and does nothing.
 *
 * All the actual writing lives in [DemoSeedWriter], which has no Android dependency
 * and is exercised by a JVM unit test against the same JSON this reads.
 */
class DemoContentSeeder(
    private val context: Context,
    private val db: ContentDatabase,
) {

    private val json = Json { ignoreUnknownKeys = true }

    /** @return true when rows were written. */
    suspend fun seedIfEmpty(): Boolean = withContext(Dispatchers.IO) {
        if (db.wordsQueries.countAll().executeAsOne() > 0L) return@withContext false

        val content = runCatching {
            context.assets.open(ASSET).use { stream ->
                json.decodeFromString<DemoContent>(stream.readBytes().decodeToString())
            }
        }.onFailure { Log.e(TAG, "cannot read $ASSET", it) }.getOrNull()
            ?: return@withContext false

        val written = runCatching { DemoSeedWriter.seed(db, content) }
            .onFailure { Log.e(TAG, "demo fixture is invalid", it) }
            .getOrNull() ?: return@withContext false

        Log.i(TAG, "seeded $written demo words (${content.contentVersion})")
        true
    }

    companion object {
        private const val TAG = "DemoContentSeeder"
        const val ASSET = "demo/demo_content.json"
    }
}
