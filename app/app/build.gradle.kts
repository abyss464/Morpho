import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.android)
    alias(libs.plugins.kotlin.compose)
    alias(libs.plugins.kotlin.serialization)
    alias(libs.plugins.sqldelight)
}

android {
    namespace = "dev.morpho"
    compileSdk = libs.versions.compileSdk.get().toInt()
    buildToolsVersion = libs.versions.buildTools.get()

    defaultConfig {
        applicationId = "dev.morpho"
        minSdk = libs.versions.minSdk.get().toInt()
        targetSdk = libs.versions.targetSdk.get().toInt()
        versionCode = 1
        versionName = "0.4.0-wave3b"

        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
        vectorDrawables.useSupportLibrary = true
    }

    buildTypes {
        debug {
            applicationIdSuffix = ".debug"
            versionNameSuffix = "-debug"
            isMinifyEnabled = false
        }
        release {
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro",
            )
            // Wave 1 has no signing config; `assembleRelease` produces an unsigned APK.
        }
    }

    // Distribution flavors mirror README Part 5: Play Asset Delivery vs. one fat APK.
    // Both read media through the same `AssetContentStore` at `content_media/…`; only
    // the packaging differs.
    flavorDimensions += "distribution"
    productFlavors {
        create("pad") {
            dimension = "distribution"
            isDefault = true
        }
        create("fatApk") {
            dimension = "distribution"
        }
    }

    // The install-time pack is consumed by `bundle*` tasks only — APK assembly ignores
    // asset packs entirely, which is exactly why the fatApk flavour needs its own route
    // to the same bytes (below) and why declaring the pack here leaves fatApk APKs
    // untouched. AGP offers no per-flavour assetPacks DSL, so this is the honest split.
    assetPacks += ":content_media"

    buildFeatures {
        compose = true
        buildConfig = true
    }

    packaging {
        resources {
            excludes += "/META-INF/{AL2.0,LGPL2.1}"
        }
        // Media assets are already compressed (webp/ogg) and must stay uncompressed so
        // Media3 can play them through a zero-copy AssetFileDescriptor.
        jniLibs.useLegacyPackaging = false
    }

    androidResources {
        noCompress += listOf("webp", "ogg", "wav", "db")
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
        isCoreLibraryDesugaringEnabled = false
    }

    sourceSets {
        getByName("main") {
            java.srcDirs("src/main/kotlin")
        }
        getByName("test") {
            java.srcDirs("src/test/kotlin")
        }
        getByName("androidTest") {
            java.srcDirs("src/androidTest/kotlin")
        }
        // One fat APK carries the media in its own assets. Pointed at the asset pack's
        // source directory rather than a second copy, so wave 3b unpacks the export
        // once and both distributions pick it up.
        getByName("fatApk") {
            assets.srcDirs("../content_media/src/main/assets")
        }
    }

    testOptions {
        unitTests {
            isIncludeAndroidResources = true
            isReturnDefaultValues = true
            all { test -> test.useJUnit() }
        }
    }

    lint {
        abortOnError = false
        warningsAsErrors = false
    }
}

kotlin {
    // See domain/build.gradle.kts: explicit jvmTarget instead of a toolchain so the
    // project builds on any JDK 17+ without toolchain provisioning.
    compilerOptions {
        jvmTarget.set(JvmTarget.JVM_17)
    }
}

sqldelight {
    databases {
        // release.db — read-only, produced by `morphod export`.
        // The .sq files mirror docs/contracts/release-db.sql exactly.
        create("ContentDatabase") {
            packageName.set("dev.morpho.data.db.content")
            srcDirs.setFrom("src/main/sqldelight/content")
            // release.db is an external artifact; SQLDelight owns no migrations for it,
            // and its generated Schema never creates the file -- DatabaseProvider copies
            // the bundled asset in and restamps user_version so the helper stays out.
            verifyMigrations.set(false)
            deriveSchemaFromMigrations.set(false)
            generateAsync.set(false)
        }
        // user.db — read/write, owned by the app.
        // The .sq files mirror docs/contracts/user-db.sql exactly.
        create("UserDatabase") {
            packageName.set("dev.morpho.data.db.user")
            srcDirs.setFrom("src/main/sqldelight/user")
            verifyMigrations.set(false)
            deriveSchemaFromMigrations.set(false)
            generateAsync.set(false)
        }
    }
}

dependencies {
    implementation(project(":domain"))

    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.activity.compose)
    implementation(libs.androidx.lifecycle.runtime.ktx)
    implementation(libs.androidx.lifecycle.runtime.compose)
    implementation(libs.androidx.lifecycle.viewmodel.compose)
    implementation(libs.androidx.navigation.compose)

    val composeBom = platform(libs.androidx.compose.bom)
    implementation(composeBom)
    androidTestImplementation(composeBom)
    implementation(libs.androidx.compose.ui)
    implementation(libs.androidx.compose.ui.graphics)
    implementation(libs.androidx.compose.ui.tooling.preview)
    implementation(libs.androidx.compose.material3)
    implementation(libs.androidx.compose.material.icons.extended)
    debugImplementation(libs.androidx.compose.ui.tooling)
    debugImplementation(libs.androidx.compose.ui.test.manifest)

    implementation(libs.sqldelight.android.driver)
    implementation(libs.sqldelight.coroutines.extensions)
    implementation(libs.sqldelight.primitive.adapters)

    implementation(libs.androidx.media3.exoplayer)
    implementation(libs.androidx.media3.common)
    implementation(libs.androidx.media3.datasource)
    implementation(libs.coil.compose)

    implementation(libs.kotlinx.coroutines.android)
    implementation(libs.kotlinx.serialization.json)

    testImplementation(libs.junit)
    testImplementation(libs.kotlin.test)
    testImplementation(libs.kotlinx.coroutines.test)
    // JDBC SQLite driver so JVM unit tests can open the real bundled release.db and
    // run the generated queries against it -- no device, no Robolectric.
    testImplementation(libs.sqldelight.sqlite.driver)

    androidTestImplementation(libs.androidx.test.junit)
    androidTestImplementation(libs.androidx.test.espresso.core)
    androidTestImplementation(libs.androidx.compose.ui.test.junit4)
}

tasks.withType<Test>().configureEach {
    testLogging {
        events("passed", "failed", "skipped")
    }
}
