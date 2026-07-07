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

    // Release signing comes from CI env (decoded from repo secrets); local
    // builds without the env fall back to debug signing so `assembleRelease`
    // still works on dev machines. The keystore is permanent — sideload
    // updates require a stable signature.
    val releaseKeystore = System.getenv("VELLUM_ANDROID_KEYSTORE")
    if (releaseKeystore != null) {
        signingConfigs.create("release") {
            storeFile = file(releaseKeystore)
            storeType = "PKCS12"
            storePassword = System.getenv("VELLUM_ANDROID_KEYSTORE_PASSWORD")
            keyAlias = System.getenv("VELLUM_ANDROID_KEY_ALIAS") ?: "vellumfe"
            keyPassword = System.getenv("VELLUM_ANDROID_KEY_PASSWORD")
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            signingConfig = if (releaseKeystore != null) {
                signingConfigs.getByName("release")
            } else {
                signingConfigs.getByName("debug")
            }
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
