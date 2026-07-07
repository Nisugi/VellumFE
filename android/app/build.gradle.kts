plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

android {
    namespace = "dev.vellumfe"
    compileSdk = 35

    defaultConfig {
        applicationId = "dev.vellumfe"
        // minSdk 26 = Android 8.0: adaptive icons + startForegroundService,
        // and it covers the oldest hardware we care about (Galaxy S7 era).
        minSdk = 26
        targetSdk = 35
        versionCode = 1
        versionName = "0.1.0"
        // Must match the cargo-ndk targets that populate src/main/jniLibs:
        // arm64 for real phones, x86_64 for the Android Studio emulator.
        ndk { abiFilters += listOf("arm64-v8a", "x86_64") }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            // Debug-signed until the release keystore lands (Phase C3):
            // fine for sideload testing, but updates across CI runs need
            // an uninstall because the debug key differs per machine.
            signingConfig = signingConfigs.getByName("debug")
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
}

kotlin {
    compilerOptions {
        jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17)
    }
}
