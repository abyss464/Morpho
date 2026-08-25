package dev.morpho.di

import android.content.Context
import android.util.Log
import coil3.ImageLoader
import dev.morpho.BuildConfig
import dev.morpho.data.content.ContentStore
import dev.morpho.data.content.DemoMediaGenerator
import dev.morpho.data.content.DirectoryContentStore
import dev.morpho.data.content.FallbackContentStore
import dev.morpho.data.content.AssetContentStore
import dev.morpho.data.db.DatabaseProvider
import dev.morpho.data.haptics.HapticsManager
import dev.morpho.data.media.AudioPlayer
import dev.morpho.data.media.CoilContentImageRenderer
import dev.morpho.data.media.MorphoImageLoader
import dev.morpho.data.repository.ContentRepository
import dev.morpho.data.repository.ProgressRepository
import dev.morpho.data.repository.SettingsRepository
import dev.morpho.data.seed.DemoContentSeeder
import dev.morpho.data.sound.SfxEvent
import dev.morpho.data.sound.SoundManager
import dev.morpho.domain.review.FsrsScheduler
import dev.morpho.domain.review.ReviewScheduler
import dev.morpho.ui.designsystem.component.ContentImageRenderer
import java.io.File
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/**
 * Manual dependency container.
 *
 * No Hilt by convention (docs/contracts/conventions.md): the graph is small enough
 * that an explicit object beats an annotation processor, and every wire is visible in
 * one file.
 */
class AppContainer(private val context: Context) {

    // --- storage ------------------------------------------------------------

    val databaseProvider: DatabaseProvider by lazy { DatabaseProvider(context) }

    val contentRepository: ContentRepository by lazy {
        ContentRepository(databaseProvider.contentDatabase)
    }

    val progressRepository: ProgressRepository by lazy {
        ProgressRepository(databaseProvider.userDatabase)
    }

    val settingsRepository: SettingsRepository by lazy {
        SettingsRepository(progressRepository)
    }

    // --- media --------------------------------------------------------------

    /** Placeholder media the demo seeder's filenames point at. */
    private val demoMediaDir: File by lazy {
        File(context.filesDir, DEMO_MEDIA_DIR).apply { mkdirs() }
    }

    /**
     * Shipping builds read `assets/content_media`; wave 1 has no such directory, so
     * the generated demo media answers instead. Ordering the asset store first means
     * dropping in real media later needs no code change.
     */
    val contentStore: ContentStore by lazy {
        FallbackContentStore(
            listOf(
                AssetContentStore(context),
                DirectoryContentStore(demoMediaDir),
            ),
        )
    }

    val audioPlayer: AudioPlayer by lazy { AudioPlayer(context, contentStore) }

    val soundManager: SoundManager by lazy {
        SoundManager(context, isContentAudioPlaying = { audioPlayer.isPlaying() })
    }

    val hapticsManager: HapticsManager by lazy { HapticsManager(context) }

    val imageLoader: ImageLoader by lazy { MorphoImageLoader.create(context, contentStore) }

    val contentImageRenderer: ContentImageRenderer by lazy {
        CoilContentImageRenderer(imageLoader)
    }

    // --- domain -------------------------------------------------------------

    val fsrs: FsrsScheduler by lazy { FsrsScheduler() }

    val reviewScheduler: ReviewScheduler by lazy { ReviewScheduler(fsrs) }

    /** Results of the last finished session, read by the summary screen. */
    val sessionResults: SessionResultHolder = SessionResultHolder()

    // --- lifecycle ----------------------------------------------------------

    /**
     * One-time startup work, run off the main thread before the first frame needs data:
     * seed the demo release, generate placeholder media, reconcile the content version,
     * load settings and preload the SFX.
     */
    suspend fun initialize(): StartupReport = withContext(Dispatchers.IO) {
        val seeded = DemoContentSeeder(context, databaseProvider.contentDatabase).seedIfEmpty()
        if (seeded) contentRepository.invalidate()

        val manifest = contentRepository.allMediaFiles()
        val generated = DemoMediaGenerator(demoMediaDir)
            .ensure(imageFiles = manifest.images, audioFiles = manifest.audio)

        val contentVersion = contentRepository.contentVersion()
        progressRepository.ensureInitialised(contentVersion)

        val settings = settingsRepository.load()
        soundManager.enabled = settings.soundEnabled
        soundManager.volume = settings.sfxVolume
        hapticsManager.enabled = settings.hapticsEnabled
        soundManager.preload(context)

        val violations = if (BuildConfig.DEBUG) {
            contentRepository.assertIntegrity()
        } else {
            emptyList()
        }
        violations.forEach { Log.e(TAG, "release integrity: $it") }

        StartupReport(
            wordCount = contentRepository.wordCount(),
            contentVersion = contentVersion,
            seededDemoContent = seeded,
            generatedMediaFiles = generated,
            integrityViolations = violations,
        )
    }

    fun playSfx(event: SfxEvent) = soundManager.play(event)

    fun shutdown() {
        audioPlayer.release()
        soundManager.release()
        databaseProvider.close()
    }

    companion object {
        private const val TAG = "AppContainer"
        const val DEMO_MEDIA_DIR = "demo_media"
    }
}

data class StartupReport(
    val wordCount: Int,
    val contentVersion: String?,
    val seededDemoContent: Boolean,
    val generatedMediaFiles: Int,
    val integrityViolations: List<String>,
)

/** Tiny in-memory hand-off between the session screens and the summary screen. */
class SessionResultHolder {
    @Volatile
    var last: SessionResult? = null
        private set

    fun publish(result: SessionResult) {
        last = result
    }

    fun consume(): SessionResult? = last
}

data class SessionResult(
    val kind: SessionKind,
    val newLearned: Int,
    val reviewed: Int,
    val correctFirstTry: Int,
    val totalFirstTry: Int,
    val streakDays: Int,
    val goalMet: Boolean,
) {
    val accuracy: Float?
        get() = if (totalFirstTry == 0) null else correctFirstTry.toFloat() / totalFirstTry
}

enum class SessionKind { LEARNING, REVIEW }
