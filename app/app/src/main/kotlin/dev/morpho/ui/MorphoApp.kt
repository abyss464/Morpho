package dev.morpho.ui

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.navigation.NavHostController
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.rememberNavController
import dev.morpho.di.AppContainer
import dev.morpho.di.StartupReport
import dev.morpho.ui.designsystem.component.MorphoLoader
import dev.morpho.ui.designsystem.motion.rememberSharedAxis
import dev.morpho.ui.settings.SettingsScreen
import dev.morpho.ui.stream.StreamScreen
import dev.morpho.ui.today.TodayScreen

/** Navigation graph. Single activity, one Compose NavHost: Today, the stream, settings. */
object MorphoRoutes {
    const val TODAY = "today"
    const val STREAM = "stream"
    const val SETTINGS = "settings"
}

@Composable
fun MorphoApp(
    container: AppContainer,
    startup: StartupReport?,
    navController: NavHostController = rememberNavController(),
) {
    Surface(
        modifier = Modifier.fillMaxSize(),
        color = MaterialTheme.colorScheme.background,
    ) {
        if (startup == null) {
            Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                MorphoLoader()
            }
            return@Surface
        }

        // Captured once: NavHost's transition lambdas run outside composition.
        val axis = rememberSharedAxis()

        NavHost(
            navController = navController,
            startDestination = MorphoRoutes.TODAY,
            enterTransition = { axis.enter(forward = true) },
            exitTransition = { axis.exit(forward = true) },
            popEnterTransition = { axis.enter(forward = false) },
            popExitTransition = { axis.exit(forward = false) },
        ) {
            composable(MorphoRoutes.TODAY) {
                TodayScreen(
                    container = container,
                    onOpenStream = { navController.navigate(MorphoRoutes.STREAM) },
                    onOpenSettings = { navController.navigate(MorphoRoutes.SETTINGS) },
                )
            }

            composable(MorphoRoutes.STREAM) {
                StreamScreen(
                    container = container,
                    onExit = { navController.popBackStack(MorphoRoutes.TODAY, inclusive = false) },
                )
            }

            composable(MorphoRoutes.SETTINGS) {
                SettingsScreen(
                    container = container,
                    startup = startup,
                    onBack = { navController.popBackStack() },
                )
            }
        }
    }
}
