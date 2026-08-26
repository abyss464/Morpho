// Root build file for the Morpho Android project.
// Plugins are declared here without applying them; modules opt in.
plugins {
    alias(libs.plugins.android.application) apply false
    alias(libs.plugins.android.asset.pack) apply false
    alias(libs.plugins.kotlin.android) apply false
    alias(libs.plugins.kotlin.jvm) apply false
    alias(libs.plugins.kotlin.compose) apply false
    alias(libs.plugins.kotlin.serialization) apply false
    alias(libs.plugins.sqldelight) apply false
}
