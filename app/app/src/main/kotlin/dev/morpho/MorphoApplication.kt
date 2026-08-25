package dev.morpho

import android.app.Application
import dev.morpho.di.AppContainer

/**
 * Owns the [AppContainer] for the whole process. Single activity, single container,
 * no service locator scattered around.
 */
class MorphoApplication : Application() {

    lateinit var container: AppContainer
        private set

    override fun onCreate() {
        super.onCreate()
        container = AppContainer(this)
    }

    override fun onTerminate() {
        // Not called on real devices, but keeps instrumentation runs tidy.
        container.shutdown()
        super.onTerminate()
    }
}
