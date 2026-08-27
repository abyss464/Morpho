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
import dev.morpho.di.SessionKind
import dev.morpho.di.StartupReport
import dev.morpho.ui.designsystem.component.MorphoLoader
import dev.morpho.ui.designsystem.motion.rememberSharedAxis
import dev.morpho.ui.home.HomeScreen
import dev.morpho.ui.learn.LearnScreen
import dev.morpho.ui.review.ReviewScreen
import dev.morpho.ui.settings.SettingsScreen
import dev.morpho.ui.summary.SessionSummaryScreen

/** Navigation graph. Single activity, one Compose NavHost. */
object MorphoRoutes {
    const val HOME = "home"
    const val LEARN = "learn"
    const val REVIEW = "review"
    const val SETTINGS = "settings"
    const val SUMMARY = "summary/{kind}"

    fun summary(kind: SessionKind) = "summary/${kind.name}"
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
            startDestination = MorphoRoutes.HOME,
            enterTransition = { axis.enter(forward = true) },
            exitTransition = { axis.exit(forward = true) },
            popEnterTransition = { axis.enter(forward = false) },
            popExitTransition = { axis.exit(forward = false) },
        ) {
            composable(MorphoRoutes.HOME) {
                HomeScreen(
                    container = container,
                    startup = startup,
                    onStartLearning = { navController.navigate(MorphoRoutes.LEARN) },
                    onStartReview = { navController.navigate(MorphoRoutes.REVIEW) },
                    onOpenSettings = { navController.navigate(MorphoRoutes.SETTINGS) },
                )
            }

            composable(MorphoRoutes.LEARN) {
                LearnScreen(
                    container = container,
                    onFinished = {
                        navController.navigate(MorphoRoutes.summary(SessionKind.LEARNING)) {
                            popUpTo(MorphoRoutes.HOME)
                        }
                    },
                    onExit = { navController.popBackStack() },
                )
            }

            composable(MorphoRoutes.REVIEW) {
                ReviewScreen(
                    container = container,
                    onFinished = {
                        navController.navigate(MorphoRoutes.summary(SessionKind.REVIEW)) {
                            popUpTo(MorphoRoutes.HOME)
                        }
                    },
                    onExit = { navController.popBackStack() },
                )
            }

            composable(MorphoRoutes.SUMMARY) { entry ->
                val kind = entry.arguments?.getString("kind")
                    ?.let { runCatching { SessionKind.valueOf(it) }.getOrNull() }
                    ?: SessionKind.LEARNING
                SessionSummaryScreen(
                    container = container,
                    kind = kind,
                    onBackHome = {
                        navController.popBackStack(MorphoRoutes.HOME, inclusive = false)
                    },
                    onKeepGoing = {
                        navController.navigate(MorphoRoutes.LEARN) {
                            popUpTo(MorphoRoutes.HOME)
                        }
                    },
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
