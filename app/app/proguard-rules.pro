# Morpho R8 rules.
#
# The app has no reflection-heavy frameworks, so the list is short. Everything here
# exists because a library reaches for a class name at runtime.

# --- kotlinx.serialization ---------------------------------------------------
# The plugin generates serializers as nested objects reached by name.
-keepattributes *Annotation*, InnerClasses
-dontnote kotlinx.serialization.**
-keepclassmembers class dev.morpho.data.seed.** {
    *** Companion;
}
-keepclasseswithmembers class dev.morpho.data.seed.** {
    kotlinx.serialization.KSerializer serializer(...);
}
-keep,includedescriptorclasses class dev.morpho.data.seed.**$$serializer { *; }

# --- Media3 / ExoPlayer ------------------------------------------------------
# Renderers and extractors are instantiated reflectively by name.
-dontwarn androidx.media3.**
-keep class androidx.media3.exoplayer.** { *; }

# --- SQLDelight / SQLite -----------------------------------------------------
-dontwarn app.cash.sqldelight.**
-keep class app.cash.sqldelight.driver.android.** { *; }

# --- Coil 3 ------------------------------------------------------------------
-dontwarn coil3.**

# --- Compose -----------------------------------------------------------------
# Keep @Preview functions out of release builds entirely; nothing references them.
-assumenosideeffects class androidx.compose.ui.tooling.preview.Preview

# Keep line numbers so a release crash report is readable.
-keepattributes SourceFile,LineNumberTable
-renamesourcefileattribute SourceFile
