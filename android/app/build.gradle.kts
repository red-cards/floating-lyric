plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

android {
    namespace = "dev.minilyrics.android"
    compileSdk = 34

    defaultConfig {
        applicationId = "dev.minilyrics.android"
        minSdk = 26
        targetSdk = 34
        versionCode = 2
        versionName = "0.2.0"
    }

    buildFeatures {
        buildConfig = true
    }

    // Debug keystore yang konsisten antar build (CI & lokal), supaya APK baru
    // bisa di-install menimpa yang lama tanpa uninstall. Path diambil dari env
    // MINILYRICS_DEBUG_KEYSTORE (CI mengisinya dari secret DEBUG_KEYSTORE_BASE64).
    // Kalau tidak di-set, AGP memakai ~/.android/debug.keystore bawaan mesin.
    // Password & alias default = konvensi debug keystore Android.
    signingConfigs {
        getByName("debug") {
            val path = System.getenv("MINILYRICS_DEBUG_KEYSTORE")
            if (!path.isNullOrBlank()) {
                val ks = file(path)
                require(ks.isFile) { "MINILYRICS_DEBUG_KEYSTORE tidak menunjuk ke file: $path" }
                storeFile = ks
                storePassword = System.getenv("MINILYRICS_DEBUG_KEYSTORE_PASSWORD") ?: "android"
                keyAlias = System.getenv("MINILYRICS_DEBUG_KEY_ALIAS") ?: "androiddebugkey"
                keyPassword = System.getenv("MINILYRICS_DEBUG_KEY_PASSWORD") ?: "android"
            }
        }
    }

    buildTypes {
        release {
            // Prototype untuk sideload: belum ada signing config release.
            isMinifyEnabled = false
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    kotlinOptions {
        jvmTarget = "17"
    }

    testOptions {
        // android.util.Log di unit test JVM jadi no-op (bukan exception).
        unitTests.isReturnDefaultValues = true
        unitTests.all { it.environment("LRCLIB_LIVE", System.getenv("LRCLIB_LIVE") ?: "") }
    }
}

dependencies {
    implementation("androidx.core:core-ktx:1.13.1")
    implementation("androidx.media:media:1.7.0")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.8.1")

    testImplementation("junit:junit:4.13.2")
    // org.json asli untuk unit test (yang di android.jar hanya stub).
    testImplementation("org.json:json:20240303")
}
