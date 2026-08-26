package dev.morpho.di

import android.content.Context
import android.util.Log
import coil3.ImageLoader
import dev.morpho.BuildConfig
import dev.morpho.data.backup.ProgressBackup
import dev.morpho.data.content.ContentStore
import dev.morpho.data.content.AssetContentStore
import dev.morpho.data.db.DatabaseProvider
import dev.morpho.data.haptics.HapticsManager
import dev.morpho.data.media.AudioPlayer
import dev.morpho.data.media.CoilContentImageRenderer
import dev.morpho.data.media.MorphoImageLoader
import dev.morpho.data.repository.ContentRepository
import dev.morpho.data.repository.ProgressRepository
import dev.morpho.data.repository.SettingsRepository
import dev.morpho.data.sound.SfxEvent
import dev.morpho.data.sound.SoundManager
import dev.morpho.domain.content.GlossIndex
import dev.morpho.domain.review.FsrsScheduler
import dev.morpho.domain.review.ReviewScheduler
import dev.morpho.ui.designsystem.component.ContentImageRenderer
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

    val progressBackup: ProgressBackup by lazy {
        ProgressBackup(context, databaseProvider)
    }

    // --- media --------------------------------------------------------------

    /**
     * Every image and audio clip the release references, read straight out of
     * `assets/content_media/` — the install-time asset pack on the pad flavour, the
     * APK's own assets on fatApk. One implementation, because both resolve through the
     * same `AssetManager` namespace.
     */
    val contentStore: ContentStore by lazy { AssetContentStore(context) }

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
     * open the bundled release, load the gloss anchors, reconcile the content version,
     * load settings and preload the SFX.
     *
     * Touching [ContentRepository.wordCount] here is what forces `release.db` to be
     * installed out of `assets/` and opened, so a broken bundle surfaces on the home
     * screen rather than three taps into a session.
     */
    suspend fun initialize(): StartupReport = withContext(Dispatchers.IO) {
        val contentVersion = contentRepository.contentVersion()
        progressRepository.ensureInitialised(contentVersion)

        val glossIndex = contentRepository.glossIndex()

        val settings = settingsRepository.load()
        soundManager.enabled = settings.soundEnabled
        soundManager.volume = settings.sfxVolume
        hapticsManager.enabled = settings.hapticsEnabled
        soundManager.preload(context)

        // Debug builds run the full assertion scan once (README Part 6). It is a handful
        // of grouped queries over 4k rows, not a row-by-row walk, so it stays cheap even
        // at release scale.
        val violations = if (BuildConfig.DEBUG) {
            contentRepository.assertIntegrity()
        } else {
            emptyList()
        }
        violations.forEach { Log.e(TAG, "release integrity: $it") }

        val wordCount = contentRepository.wordCount()
        Log.i(TAG, "release $contentVersion: $wordCount words, ${glossIndex.size} gloss anchors")

        StartupReport(
            wordCount = wordCount,
            contentVersion = contentVersion,
            glossIndex = glossIndex,
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
    }
}

data class StartupReport(
    val wordCount: Int,
    val contentVersion: String?,
    /** `gloss_anchors`, hoisted into a composition local by [dev.morpho.MainActivity]. */
    val glossIndex: GlossIndex = GlossIndex.EMPTY,
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
