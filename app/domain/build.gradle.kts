import org.jetbrains.kotlin.gradle.dsl.JvmTarget

// Pure-Kotlin domain module. Deliberately has NO Android dependencies so the
// learning engine, FSRS scheduler and progress tracker stay unit-testable on the
// plain JVM (docs/contracts/conventions.md, "Architecture per README Part 6").
// Only the Kotlin stdlib and java.time are used.
plugins {
    alias(libs.plugins.kotlin.jvm)
}

// Java 17 bytecode without a toolchain declaration: Gradle toolchain resolution
// needs either a matching local JDK or a provisioning repository, and neither is
// guaranteed on a build machine. Any JDK 17+ can build this as written.
java {
    sourceCompatibility = JavaVersion.VERSION_17
    targetCompatibility = JavaVersion.VERSION_17
}

kotlin {
    compilerOptions {
        jvmTarget.set(JvmTarget.JVM_17)
    }
}

dependencies {
    testImplementation(libs.junit)
    testImplementation(libs.kotlin.test)
    testImplementation(libs.kotlin.test.junit)
}

tasks.withType<Test>().configureEach {
    useJUnit()
    testLogging {
        events("passed", "failed", "skipped")
    }
}
