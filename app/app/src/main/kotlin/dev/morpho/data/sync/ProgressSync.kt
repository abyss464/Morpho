package dev.morpho.data.sync

import android.util.Log
import dev.morpho.data.repository.SettingsRepository
import dev.morpho.data.stream.StreamSnapshot
import dev.morpho.data.stream.StreamStore
import dev.morpho.data.stream.WordNote
import dev.morpho.domain.model.CardState
import dev.morpho.domain.model.FsrsCard
import dev.morpho.domain.stream.StepKind
import dev.morpho.domain.stream.StreamState
import dev.morpho.domain.stream.WordStage
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.Json
import java.io.IOException
import java.net.ConnectException
import java.net.HttpURLConnection
import java.net.InetSocketAddress
import java.net.Socket
import java.net.SocketTimeoutException
import java.net.URI
import java.time.Instant
import java.time.LocalDate
import java.time.temporal.ChronoUnit

/** Why a sync did not go through. */
enum class SyncFailure {
    /** What was typed is not a web address. */
    BAD_ADDRESS,

    /** Nothing answered at the address: no network, no route, an unknown host. */
    UNREACHABLE,

    /** The computer answered, but nothing listens on that port. */
    REFUSED,

    /** The address took too long to connect or to reply. */
    TIMED_OUT,

    /** The web app answered with an HTTP error. */
    SERVER_ERROR,

    /** The reply was not a version-1 progress document. */
    BAD_REPLY,
}

/** How the last sync went. */
sealed interface SyncOutcome {
    /** [updated] words changed here from the web; [sent] words went to the web. */
    data class Done(val updated: Int, val sent: Int) : SyncOutcome

    data class Failed(val reason: SyncFailure, val address: String, val status: Int? = null) : SyncOutcome
}

/**
 * Progress sync with the web app (docs/contracts/sync.md). The web app's server merges;
 * this sends the local document — every card, every stream stage, the notes — and applies
 * the merged one it gets back. Today's counters, `daily_stats` and settings stay local.
 */
class ProgressSync(
    private val store: StreamStore,
    private val settings: SettingsRepository,
) {
    private val json = Json {
        ignoreUnknownKeys = true
        encodeDefaults = true
    }
    private val running = Mutex()

    private val _last = MutableStateFlow<SyncOutcome?>(null)

    /** The most recent sync's outcome in this run of the app, or null before the first. */
    val last: StateFlow<SyncOutcome?> = _last.asStateFlow()

    /** Saves [address] as the sync address and syncs with it. */
    suspend fun syncWith(address: String): SyncOutcome {
        val base = normalizeAddress(address)
            ?: return SyncOutcome.Failed(SyncFailure.BAD_ADDRESS, address.trim()).also { _last.value = it }
        settings.setSyncAddress(base)
        return sync(base)
    }

    /** Syncs with the saved address; null when none is saved. */
    suspend fun syncSaved(): SyncOutcome? = settings.settings.value.syncAddress?.let { sync(it) }

    private suspend fun sync(base: String): SyncOutcome = running.withLock {
        val outcome = try {
            val snapshot = store.open(LocalDate.now())
            val notes = store.notes()
            val local = export(snapshot, notes)
            val reply = post(base, json.encodeToString(SyncDocument.serializer(), local))
            val merged = runCatching { json.decodeFromString(SyncDocument.serializer(), reply) }
                .getOrNull()
                ?.takeIf { it.v == SyncDocument.VERSION }
            if (merged == null) {
                SyncOutcome.Failed(SyncFailure.BAD_REPLY, base)
            } else {
                // Read again under the lock: a step finished while the request was out still counts.
                val fresh = store.open(LocalDate.now())
                SyncOutcome.Done(updated = apply(fresh, store.notes(), merged), sent = local.words.size)
            }
        } catch (e: HttpStatus) {
            SyncOutcome.Failed(SyncFailure.SERVER_ERROR, base, e.code)
        } catch (e: SocketTimeoutException) {
            SyncOutcome.Failed(SyncFailure.TIMED_OUT, base)
        } catch (e: ConnectException) {
            SyncOutcome.Failed(if (refused(base)) SyncFailure.REFUSED else SyncFailure.UNREACHABLE, base)
        } catch (e: IOException) {
            Log.w(TAG, "sync with $base failed", e)
            SyncOutcome.Failed(SyncFailure.UNREACHABLE, base)
        } catch (e: IllegalArgumentException) {
            SyncOutcome.Failed(SyncFailure.BAD_ADDRESS, base)
        }
        _last.value = outcome
        outcome
    }

    // ------------------------------------------------------------- document

    private fun export(snapshot: StreamSnapshot, notes: Map<Long, WordNote>): SyncDocument {
        val ids = snapshot.cards.keys + snapshot.state.words.keys
        val words = ids.associate { id ->
            id.toString() to SyncEntry(
                card = snapshot.cards[id]?.let(::toSync),
                stage = snapshot.state.words[id]?.let(::toSync),
            )
        }
        return SyncDocument(
            words = words,
            notes = notes.entries.associate { (id, n) -> id.toString() to SyncNote(n.text, iso(n.at)) },
        )
    }

    /**
     * Applies the merged document (contract §4) in one transaction: for each word whose
     * merged entry differs, its card is set and its stage replaced or removed (an imported
     * stage starts its spacing now); the step on screen and the last review are dropped
     * when their word changed; newer notes are taken. Returns how many words changed.
     */
    private suspend fun apply(snapshot: StreamSnapshot, notes: Map<Long, WordNote>, merged: SyncDocument): Int {
        val state = snapshot.state
        val words = state.words.toMutableMap()
        val cards = mutableListOf<FsrsCard>()
        var current = state.current
        var lastReview = state.lastReview
        var changed = 0
        for ((key, sent) in merged.words) {
            val id = key.toLongOrNull() ?: continue
            val entry = sent.copy(stage = sent.stage?.normalized())
            val local = SyncEntry(snapshot.cards[id]?.let(::toSync), state.words[id]?.let(::toSync))
            if (sameEntry(entry, local)) continue
            val card = entry.card?.let { runCatching { fromSync(id, it) }.getOrNull() }
            val stage = entry.stage?.let { runCatching { fromSync(it, state.seq) }.getOrNull() }
            if ((entry.card != null && card == null) || (entry.stage != null && stage == null)) {
                Log.w(TAG, "skipping word $id: unreadable entry $entry")
                continue
            }
            changed += 1
            if (card != null) cards += card
            if (stage != null) words[id] = stage else words.remove(id)
            if (current?.wordId == id) current = null
            if (lastReview?.wordId == id) lastReview = null
        }
        val newNotes = notes.toMutableMap()
        var notesChanged = false
        for ((key, note) in merged.notes) {
            val id = key.toLongOrNull() ?: continue
            val at = runCatching { Instant.parse(note.at) }.getOrNull() ?: continue
            val old = notes[id]
            if (old == null || at.isAfter(old.at)) {
                newNotes[id] = WordNote(note.text, at)
                notesChanged = true
            }
        }
        if (changed > 0 || notesChanged) {
            val next: StreamState = state.copy(words = words, current = current, lastReview = lastReview)
            store.replace(cards, next, newNotes)
        }
        return changed
    }

    // ------------------------------------------------------------- network

    private class HttpStatus(val code: Int) : IOException("HTTP $code")

    /**
     * Whether [base]'s host refuses connections on its port. The platform's HTTP stack reports
     * every failed connect as "Failed to connect to …"; a plain socket names ECONNREFUSED.
     */
    private suspend fun refused(base: String): Boolean = withContext(Dispatchers.IO) {
        try {
            val uri = URI(base)
            val port = if (uri.port != -1) uri.port else if (uri.scheme.equals("https", ignoreCase = true)) 443 else 80
            Socket().use { it.connect(InetSocketAddress(uri.host, port), CONNECT_TIMEOUT_MS) }
            false
        } catch (e: ConnectException) {
            e.message.orEmpty().contains("refused", ignoreCase = true)
        } catch (e: IOException) {
            false
        } catch (e: IllegalArgumentException) {
            false
        }
    }

    private suspend fun post(base: String, body: String): String = withContext(Dispatchers.IO) {
        val connection = URI("$base$PATH").toURL().openConnection() as HttpURLConnection
        try {
            connection.requestMethod = "POST"
            connection.connectTimeout = CONNECT_TIMEOUT_MS
            connection.readTimeout = READ_TIMEOUT_MS
            connection.doOutput = true
            connection.setRequestProperty("Content-Type", "application/json; charset=utf-8")
            connection.setRequestProperty("Accept", "application/json")
            connection.outputStream.use { it.write(body.toByteArray(Charsets.UTF_8)) }
            val status = connection.responseCode
            if (status !in 200..299) throw HttpStatus(status)
            connection.inputStream.bufferedReader(Charsets.UTF_8).use { it.readText() }
        } finally {
            connection.disconnect()
        }
    }

    companion object {
        private const val TAG = "ProgressSync"
        private const val PATH = "/api/sync"
        private const val CONNECT_TIMEOUT_MS = 5_000
        private const val READ_TIMEOUT_MS = 15_000

        /** The web app on this computer, reached over `adb reverse tcp:30017 tcp:30017`. */
        const val DEFAULT_ADDRESS = "http://127.0.0.1:30017"

        /**
         * "192.168.1.5:30017" -> "http://192.168.1.5:30017": the scheme is optional, a
         * trailing slash dropped. Null when what is left is not an http(s) address.
         */
        fun normalizeAddress(input: String): String? {
            val trimmed = input.trim().trimEnd('/')
            if (trimmed.isEmpty()) return null
            val withScheme = if ("://" in trimmed) trimmed else "http://$trimmed"
            val uri = runCatching { URI(withScheme) }.getOrNull() ?: return null
            if (uri.scheme?.lowercase() !in setOf("http", "https") || uri.host.isNullOrEmpty()) return null
            return withScheme
        }
    }
}

// ------------------------------------------------------------------ mapping

/** ISO-8601 UTC to the millisecond, as the web writes times. */
private fun iso(at: Instant): String = at.truncatedTo(ChronoUnit.MILLIS).toString()

private fun toSync(card: FsrsCard) = SyncCard(
    due = iso(card.due),
    stability = card.stability,
    difficulty = card.difficulty,
    elapsedDays = card.elapsedDays,
    scheduledDays = card.scheduledDays,
    reps = card.reps,
    lapses = card.lapses,
    state = card.state.code,
    lastReview = card.lastReview?.let(::iso),
)

private fun toSync(stage: WordStage) = SyncStage(
    stage = SyncStage.LEARNING,
    next = stage.next.name.lowercase(),
    immediate = stage.immediate,
    flawed = stage.flawed,
    attempt = stage.attempt,
)

private fun fromSync(wordId: Long, card: SyncCard) = FsrsCard(
    wordId = wordId,
    due = Instant.parse(card.due),
    stability = card.stability,
    difficulty = card.difficulty,
    elapsedDays = card.elapsedDays,
    scheduledDays = card.scheduledDays,
    reps = card.reps,
    lapses = card.lapses,
    state = CardState.fromCode(card.state),
    lastReview = card.lastReview?.let(Instant::parse),
)

/** An imported (normalized) stage starts its spacing at the stream position [seq]. */
private fun fromSync(stage: SyncStage, seq: Int) = WordStage(
    next = StepKind.valueOf(stage.next.uppercase()).also { require(it != StepKind.REVIEW) },
    immediate = stage.immediate,
    since = seq,
    flawed = stage.flawed,
    attempt = stage.attempt,
)

/** Two entries say the same thing, whatever the date format: times compare as instants. */
private fun sameEntry(a: SyncEntry, b: SyncEntry): Boolean {
    fun millis(t: String?) = t?.let { runCatching { Instant.parse(it).toEpochMilli() }.getOrNull() }
    fun cardKey(c: SyncCard?) = c?.let {
        listOf(millis(it.due), it.stability, it.difficulty, it.elapsedDays, it.scheduledDays, it.reps, it.lapses, it.state, millis(it.lastReview))
    }
    return cardKey(a.card) == cardKey(b.card) && a.stage == b.stage
}
