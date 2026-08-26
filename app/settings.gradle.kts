pluginManagement {
    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}

dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        google()
        mavenCentral()
    }
}

rootProject.name = "Morpho"

include(":app")
include(":domain")

// Install-time Play Asset Delivery pack carrying the word images and audio.
// Empty until wave 3b drops the exported media in; see content_media/README.md.
include(":content_media")
