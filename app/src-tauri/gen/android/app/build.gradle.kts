import java.util.Properties

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("rust")
    id("com.chaquo.python")
}

// Google Play edition (android-play.sh): own launcher icon (src/play/res) and no yt-dlp in the
// bundle. The app code never starts Python there: the backend rejects links that need yt-dlp.
val playEdition = System.getenv("CONVERSOR_PLAY") == "1"

val tauriProperties = Properties().apply {
    val propFile = file("tauri.properties")
    if (propFile.exists()) {
        propFile.inputStream().use { load(it) }
    }
}

android {
    compileSdk = 36
    namespace = "es.conversorstickers.app"
    defaultConfig {
        manifestPlaceholders["usesCleartextTraffic"] = "false"
        applicationId = "es.conversorstickers.app"
        minSdk = 24
        targetSdk = 36
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
        versionCode = tauriProperties.getProperty("tauri.android.versionCode", "1").toInt()
        versionName = tauriProperties.getProperty("tauri.android.versionName", "1.0")
        ndk {
            // The release script sets CONVERSOR_ARM64_ONLY so the phone APK has no x86_64 libs;
            // debug builds (Waydroid) keep both ABIs.
            abiFilters += if (System.getenv("CONVERSOR_ARM64_ONLY") == "1") {
                listOf("arm64-v8a")
            } else {
                listOf("arm64-v8a", "x86_64")
            }
        }
    }
    // Signing data comes from the environment so the key never lives in the repo.
    // Without it the release build stays unsigned (see android-release.sh).
    val releaseKeystore = System.getenv("CONVERSOR_KEYSTORE")
    val releasePassword = System.getenv("CONVERSOR_KEYSTORE_PASSWORD")
    val releaseSigning = if (!releaseKeystore.isNullOrEmpty() && !releasePassword.isNullOrEmpty()) {
        signingConfigs.create("release") {
            storeFile = file(releaseKeystore)
            storePassword = releasePassword
            keyAlias = "conversor-stickers"
            keyPassword = releasePassword
        }
    } else {
        logger.warn(
            "CONVERSOR_KEYSTORE / CONVERSOR_KEYSTORE_PASSWORD not set: the release APK will be unsigned."
        )
        null
    }
    if (playEdition) {
        sourceSets.getByName("release").res.srcDir("src/play/res")
    }
    buildTypes {
        getByName("debug") {
            manifestPlaceholders["usesCleartextTraffic"] = "true"
            isDebuggable = true
            isJniDebuggable = true
            isMinifyEnabled = false
            packaging {                jniLibs.keepDebugSymbols.add("*/arm64-v8a/*.so")
                jniLibs.keepDebugSymbols.add("*/armeabi-v7a/*.so")
                jniLibs.keepDebugSymbols.add("*/x86/*.so")
                jniLibs.keepDebugSymbols.add("*/x86_64/*.so")
            }
        }
        getByName("release") {
            if (releaseSigning != null) signingConfig = releaseSigning
            isMinifyEnabled = true
            proguardFiles(
                *fileTree(".") { include("**/*.pro") }
                    .plus(getDefaultProguardFile("proguard-android-optimize.txt"))
                    .toList().toTypedArray()
            )
        }
    }
    kotlinOptions {
        jvmTarget = "1.8"
    }
    buildFeatures {
        buildConfig = true
    }
    packaging {
        jniLibs {
            // ffmpeg and ffprobe are executables: they must be extracted to
            // nativeLibraryDir, not left inside the APK.
            useLegacyPackaging = true
        }
    }
}

// Python 3.12+ solo tiene ABIs de 64 bits. El plugin rust crea también
// variantes de 32 bits (arm, x86) que esta app no construye: se les deja un
// ABI de 64 bits para que Chaquopy no las rechace al configurar.
android {
    productFlavors {
        listOf("arm", "x86").forEach { nombre ->
            getByName(nombre) {
                ndk {
                    abiFilters.clear()
                    abiFilters += "x86_64"
                }
            }
        }
    }
}

// yt-dlp is pinned by version and hash (requirements-android.txt). Python 3.14 is the one of the
// build machine, which Chaquopy requires for pip too; yt-dlp needs 3.11 or later.
chaquopy {
    defaultConfig {
        version = "3.14"
        // The Play edition leaves yt-dlp out; link_download.py already tolerates its absence.
        if (!playEdition) {
            pip {
                install("-r", "requirements-android.txt")
            }
        }
    }
}

// The binaries are not in git: ffmpeg-android/build.sh generates them.
val checkFfmpeg by tasks.registering {
    val jniLibs = file("src/main/jniLibs")
    doLast {
        val missing = listOf("arm64-v8a", "x86_64").flatMap { abi ->
            listOf("libffmpeg.so", "libffprobe.so").map { "$abi/$it" }
        }.filter { !File(jniLibs, it).isFile }
        if (missing.isNotEmpty()) {
            throw GradleException(
                "The FFmpeg binaries are missing (${missing.joinToString(", ")}): " +
                    "run app/src-tauri/ffmpeg-android/build.sh"
            )
        }
    }
}
tasks.named("preBuild") { dependsOn(checkFfmpeg) }

rust {
    rootDirRel = "../../../"
}

dependencies {
    implementation("androidx.webkit:webkit:1.14.0")
    implementation("androidx.appcompat:appcompat:1.7.1")
    implementation("androidx.activity:activity-ktx:1.10.1")
    implementation("com.google.android.material:material:1.12.0")
    implementation("androidx.lifecycle:lifecycle-process:2.10.0")
    testImplementation("junit:junit:4.13.2")
    // En las pruebas JVM, `org.json` de android.jar no hace nada.
    testImplementation("org.json:json:20240303")
    androidTestImplementation("androidx.test.ext:junit:1.1.4")
    androidTestImplementation("androidx.test.espresso:espresso-core:3.5.0")
}

apply(from = "tauri.build.gradle.kts")