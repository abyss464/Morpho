package dev.morpho.data.content

import android.content.Context
import android.content.res.AssetFileDescriptor
import android.net.Uri
import androidx.core.net.toUri
import java.io.IOException
import java.io.InputStream

/**
 * Resolves a content-addressed media filename to bytes.
 *
 * release.db only ever stores names like `img/{hash}.webp` and `audio/{hash}.ogg`
 * (docs/contracts/release-db.sql). How those names turn into bytes depends on the
 * distribution flavour, and nothing above this interface needs to know which:
 *
 *  * **pad** — `AssetManager` reading the install-time Play Asset Delivery pack
 *  * **fatApk** — the same files packed straight into `assets/`
 *
 * Both land in the same `AssetManager` namespace, so [AssetContentStore] is the only
 * implementation the product needs.
 *
 * Implementations must be safe to call from any thread and must never touch the
 * network (README Part 6: the app is fully offline).
 */
interface ContentStore {

    /** Raw bytes, or null when the name is unknown to this store. */
    fun open(name: String): InputStream?

    /**
     * File descriptor for zero-copy playback. Media3 can play an
     * [AssetFileDescriptor] without copying the file out of the APK first.
     */
    fun openFd(name: String): AssetFileDescriptor?

    /** A URI Media3/Coil can consume directly, or null when the name is unknown. */
    fun uriFor(name: String): Uri?

    fun exists(name: String): Boolean

    companion object {
        const val IMAGE_DIR = "img"
        const val AUDIO_DIR = "audio"

        /** Scheme used by the Coil fetcher and the Media3 data source. */
        const val SCHEME = "morpho"

        /** `morpho://content/img/abc.webp` — an opaque handle back into the store. */
        fun handle(name: String): Uri = "$SCHEME://content/$name".toUri()

        /** Inverse of [handle]. */
        fun nameFrom(uri: Uri): String? =
            if (uri.scheme == SCHEME) uri.path?.removePrefix("/") else null
    }
}

/**
 * Reads media out of the APK / asset pack under `assets/content_media/`.
 * This is the shipping implementation for both the pad and fatApk flavours.
 */
class AssetContentStore(
    context: Context,
    private val root: String = "content_media",
) : ContentStore {

    private val assets = context.assets

    override fun open(name: String): InputStream? = try {
        assets.open("$root/$name", android.content.res.AssetManager.ACCESS_STREAMING)
    } catch (_: IOException) {
        null
    }

    override fun openFd(name: String): AssetFileDescriptor? = try {
        assets.openFd("$root/$name")
    } catch (_: IOException) {
        null
    }

    override fun uriFor(name: String): Uri? =
        if (exists(name)) "file:///android_asset/$root/$name".toUri() else null

    override fun exists(name: String): Boolean = try {
        assets.open("$root/$name").close()
        true
    } catch (_: IOException) {
        false
    }
}
