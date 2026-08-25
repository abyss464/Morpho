package dev.morpho

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import dev.morpho.di.AppContainer
import dev.morpho.di.StartupReport
import dev.morpho.ui.MorphoApp
import dev.morpho.ui.designsystem.component.LocalContentImageRenderer
import dev.morpho.ui.designsystem.theme.MorphoTheme

/**
 * The single activity. Everything above this is Compose and
 * `androidx.navigation.compose`.
 */
class MainActivity : ComponentActivity() {

    private lateinit var container: AppContainer

    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        container = (application as MorphoApplication).container

        setContent {
            var startup by remember { mutableStateOf<StartupReport?>(null) }
            LaunchedEffect(Unit) {
                startup = container.initialize()
            }

            val settings by container.settingsRepository.settings.collectAsStateWithLifecycle()

            MorphoTheme(forceReducedMotion = settings.reducedMotion) {
                CompositionLocalProvider(
                    LocalContentImageRenderer provides container.contentImageRenderer,
                ) {
                    MorphoApp(container = container, startup = startup)
                }
            }
        }
    }

    override fun onStop() {
        super.onStop()
        // A WAL checkpoint here keeps the Auto Backup snapshot self-contained
        // (README Part 6, "备份与迁移").
        container.databaseProvider.checkpointUserDatabase()
        container.audioPlayer.stop()
    }

    override fun onDestroy() {
        if (isFinishing) container.shutdown()
        super.onDestroy()
    }
}
