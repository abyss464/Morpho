package dev.morpho.data.media

import android.content.Context
import android.util.Log
import androidx.annotation.OptIn
import androidx.media3.common.AudioAttributes
import androidx.media3.common.C
import androidx.media3.common.MediaItem
import androidx.media3.common.Player
import androidx.media3.common.util.UnstableApi
import androidx.media3.datasource.DataSource
import androidx.media3.datasource.DataSpec
import androidx.media3.datasource.BaseDataSource
import androidx.media3.datasource.TransferListener
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.exoplayer.source.ProgressiveMediaSource
import dev.morpho.data.content.ContentStore
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import java.io.IOException
import java.io.InputStream

/**
 * Content-audio playback over Media3.
 *
 * Two players, exactly as README Part 6 specifies: a **primary** that speaks now and a
 * **preload slot** that prepares the next question's audio while the current one is on
 * screen. Media files are small Opus/WAV blobs, so preloading is effectively free and
 * the next question never opens with a stall.
 *
 * Audio is read through [ContentStore], never through a file path, so PAD and fatApk
 * both work without the player knowing which it is talking to.
 */
@OptIn(UnstableApi::class)
class AudioPlayer(
    context: Context,
    private val contentStore: ContentStore,
) {

    private val dataSourceFactory = DataSource.Factory { ContentStoreDataSource(contentStore) }

    private val attributes = AudioAttributes.Builder()
        .setUsage(C.USAGE_ASSISTANCE_ACCESSIBILITY)
        .setContentType(C.AUDIO_CONTENT_TYPE_SPEECH)
        .build()

    private fun newPlayer(ctx: Context): ExoPlayer =
        ExoPlayer.Builder(ctx)
            .setMediaSourceFactory(ProgressiveMediaSource.Factory(dataSourceFactory))
            .build()
            .apply { setAudioAttributes(attributes, /* handleAudioFocus = */ true) }

    private val primary: ExoPlayer = newPlayer(context)
    private val preload: ExoPlayer = newPlayer(context)

    private var preloadedName: String? = null

    private val _nowPlaying = MutableStateFlow<String?>(null)

    /** Filename currently sounding, or null. Screens light the matching audio chip. */
    val nowPlaying: StateFlow<String?> = _nowPlaying.asStateFlow()

    init {
        primary.addListener(object : Player.Listener {
            override fun onPlaybackStateChanged(state: Int) {
                if (state == Player.STATE_ENDED || state == Player.STATE_IDLE) {
                    _nowPlaying.value = null
                }
            }

            override fun onIsPlayingChanged(isPlaying: Boolean) {
                if (!isPlaying && primary.playbackState != Player.STATE_BUFFERING) {
                    _nowPlaying.value = null
                }
            }
        })
    }

    /** Plays [name] immediately, replacing whatever was sounding. */
    fun play(name: String?) {
        if (name.isNullOrBlank()) return
        if (!contentStore.exists(name)) {
            Log.w(TAG, "audio not found in content store: $name")
            return
        }
        runCatching {
            primary.setMediaItem(MediaItem.fromUri(ContentStore.handle(name)))
            primary.prepare()
            primary.playWhenReady = true
            _nowPlaying.value = name
        }.onFailure { Log.e(TAG, "playback failed for $name", it) }
    }

    /** Prepares [name] on the spare player so the next `play` starts instantly. */
    fun preload(name: String?) {
        if (name.isNullOrBlank() || name == preloadedName) return
        if (!contentStore.exists(name)) return
        runCatching {
            preload.setMediaItem(MediaItem.fromUri(ContentStore.handle(name)))
            preload.prepare()
            preload.playWhenReady = false
            preloadedName = name
        }.onFailure { Log.w(TAG, "preload failed for $name", it) }
    }

    fun stop() {
        primary.stop()
        _nowPlaying.value = null
    }

    /** True while content audio is sounding — [SoundManager] ducks SFX against this. */
    fun isPlaying(): Boolean = primary.isPlaying

    fun release() {
        runCatching { primary.release() }
        runCatching { preload.release() }
    }

    companion object {
        private const val TAG = "AudioPlayer"
    }
}

/**
 * Media3 data source that resolves `morpho://content/...` URIs through [ContentStore].
 *
 * Keeps the player ignorant of the distribution flavour and lets the shipping build
 * hand Media3 an `AssetFileDescriptor`-backed stream with no unpacking step.
 */
@OptIn(UnstableApi::class)
private class ContentStoreDataSource(
    private val store: ContentStore,
) : BaseDataSource(/* isNetwork = */ false) {

    private var uri: android.net.Uri? = null
    private var stream: InputStream? = null
    private var bytesRemaining: Long = C.LENGTH_UNSET.toLong()
    private var opened = false

    override fun open(dataSpec: DataSpec): Long {
        transferInitializing(dataSpec)
        uri = dataSpec.uri
        val name = ContentStore.nameFrom(dataSpec.uri)
            ?: throw IOException("not a content-store uri: ${dataSpec.uri}")
        val input = store.open(name) ?: throw IOException("missing content: $name")
        stream = input
        if (dataSpec.position > 0) input.skip(dataSpec.position)
        bytesRemaining = if (dataSpec.length != C.LENGTH_UNSET.toLong()) {
            dataSpec.length
        } else {
            val available = input.available().toLong()
            if (available > 0) available else C.LENGTH_UNSET.toLong()
        }
        opened = true
        transferStarted(dataSpec)
        return bytesRemaining
    }

    override fun read(buffer: ByteArray, offset: Int, length: Int): Int {
        if (length == 0) return 0
        if (bytesRemaining == 0L) return C.RESULT_END_OF_INPUT
        val toRead = if (bytesRemaining == C.LENGTH_UNSET.toLong()) {
            length
        } else {
            minOf(bytesRemaining, length.toLong()).toInt()
        }
        val read = stream?.read(buffer, offset, toRead) ?: -1
        if (read == -1) return C.RESULT_END_OF_INPUT
        if (bytesRemaining != C.LENGTH_UNSET.toLong()) bytesRemaining -= read
        bytesTransferred(read)
        return read
    }

    override fun getUri(): android.net.Uri? = uri

    override fun close() {
        uri = null
        runCatching { stream?.close() }
        stream = null
        if (opened) {
            opened = false
            transferEnded()
        }
    }
}

/** Unused today, kept so callers can be explicit about the no-op listener. */
@OptIn(UnstableApi::class)
internal val NoOpTransferListener: TransferListener? = null
