package dev.morpho.data.media

import android.content.Context
import androidx.compose.foundation.layout.Box
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalInspectionMode
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import coil3.ImageLoader
import coil3.PlatformContext
import coil3.decode.DataSource
import coil3.disk.DiskCache
import coil3.fetch.FetchResult
import coil3.fetch.Fetcher
import coil3.fetch.SourceFetchResult
import coil3.memory.MemoryCache
import coil3.request.ImageRequest
import coil3.request.Options
import coil3.request.crossfade
import dev.morpho.data.content.ContentStore
import dev.morpho.ui.designsystem.component.ContentImageRenderer
import dev.morpho.ui.designsystem.component.GradientContentImageRenderer
import okio.buffer
import okio.source

/**
 * Coil 3 wiring.
 *
 * The network layer is deliberately never installed: `coil-network-*` is not a
 * dependency, so there is no code path that could reach out. Images come only from
 * [ContentStore] via [ContentStoreFetcher]. The disk cache is also switched off —
 * the source bytes are already local, so a second copy would be pure waste.
 */
object MorphoImageLoader {

    fun create(context: Context, contentStore: ContentStore): ImageLoader =
        ImageLoader.Builder(context)
            .components { add(ContentStoreFetcher.Factory(contentStore)) }
            .memoryCache {
                MemoryCache.Builder()
                    .maxSizePercent(context as PlatformContext, 0.20)
                    .build()
            }
            .diskCache(null as DiskCache?)
            .crossfade(true)
            .build()
}

/** Resolves `morpho://content/img/{hash}.webp` handles to bytes. */
class ContentStoreFetcher(
    private val store: ContentStore,
    private val name: String,
    private val options: Options,
) : Fetcher {

    override suspend fun fetch(): FetchResult? {
        val stream = store.open(name) ?: return null
        return SourceFetchResult(
            source = coil3.decode.ImageSource(
                source = stream.source().buffer(),
                fileSystem = okio.FileSystem.SYSTEM,
            ),
            mimeType = null,
            dataSource = DataSource.DISK,
        )
    }

    class Factory(private val store: ContentStore) : Fetcher.Factory<coil3.Uri> {
        override fun create(data: coil3.Uri, options: Options, imageLoader: ImageLoader): Fetcher? {
            if (data.scheme != ContentStore.SCHEME) return null
            val name = data.path?.removePrefix("/") ?: return null
            return ContentStoreFetcher(store, name, options)
        }
    }
}

/**
 * The production [ContentImageRenderer]: an `AsyncImage` over the Coil loader above.
 *
 * Inside `@Preview` (`LocalInspectionMode`) it falls back to the design system's
 * gradient renderer, so previews stay asset-free and instant.
 */
class CoilContentImageRenderer(
    private val imageLoader: ImageLoader,
) : ContentImageRenderer {

    @Composable
    override fun Image(file: String, contentDescription: String?, modifier: Modifier) {
        if (LocalInspectionMode.current) {
            GradientContentImageRenderer.Image(file, contentDescription, modifier)
            return
        }
        Box(modifier.semantics { contentDescription?.let { this.contentDescription = it } }) {
            coil3.compose.AsyncImage(
                model = ImageRequest.Builder(androidx.compose.ui.platform.LocalContext.current)
                    .data(ContentStore.handle(file).toString())
                    .build(),
                imageLoader = imageLoader,
                contentDescription = null,
                contentScale = ContentScale.Crop,
                modifier = Modifier.matchParentSize(),
            )
        }
    }
}
