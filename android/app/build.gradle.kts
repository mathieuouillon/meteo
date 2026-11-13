import java.util.Properties

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
}

android {
    namespace = "dev.meteo.app"
    compileSdk = 36
    ndkVersion = "30.0.16248370"

    defaultConfig {
        applicationId = "dev.meteo.app"
        minSdk = 28
        targetSdk = 36
        versionCode = 1
        versionName = "0.1.0"
        ndk { abiFilters += "arm64-v8a" }
    }

    // Release signing: your upload key, described in android/keystore.properties
    // (never committed; see README "Release"). Without that file, release
    // builds are signed with the debug key so they still install.
    val keystoreFile = rootProject.file("keystore.properties")
    signingConfigs {
        if (keystoreFile.exists()) {
            val p = Properties().apply { keystoreFile.inputStream().use { load(it) } }
            create("upload") {
                storeFile = rootProject.file(p.getProperty("storeFile"))
                storePassword = p.getProperty("storePassword")
                keyAlias = p.getProperty("keyAlias")
                keyPassword = p.getProperty("keyPassword")
            }
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            signingConfig = signingConfigs.findByName("upload") ?: signingConfigs.getByName("debug")
        }
    }

    buildFeatures { compose = true }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions { jvmTarget = "17" }

    // The Rust libraries built by the cargoBuild task below.
    sourceSets["main"].jniLibs.srcDir(layout.buildDirectory.dir("rustJniLibs"))
    // RustBridge.kt (the JNI calls into libmeteo_widget.so), shared with the watch app.
    sourceSets["main"].java.srcDir("../shared/kotlin")
}

dependencies {
    implementation("androidx.glance:glance-appwidget:1.1.1")
    // Background checks for the notifications (NotifyWorker).
    implementation("androidx.work:work-runtime-ktx:2.10.1")
}

// Builds the Rust crates for Android with cargo-ndk:
//   libmeteo.so         the Slint app (run by android.app.NativeActivity)
//   libmeteo_widget.so  the widget's JNI bridge (no UI code)
val release = gradle.startParameter.taskNames.any { it.contains("Release", ignoreCase = true) }
val cargoBuild by tasks.registering(Exec::class) {
    description = "Builds the Rust libraries with cargo-ndk."
    workingDir = rootDir.parentFile
    val outDir = layout.buildDirectory.dir("rustJniLibs").get().asFile
    environment("ANDROID_HOME", android.sdkDirectory.path)
    environment("ANDROID_NDK_HOME", android.ndkDirectory.path)
    // Slint's build script compiles a Java helper against this android.jar.
    environment("ANDROID_JAR", File(android.sdkDirectory, "platforms/android-36/android.jar").path)
    val cargo = File(System.getProperty("user.home"), ".cargo/bin/cargo").takeIf { it.exists() }?.path ?: "cargo"
    commandLine(
        listOf(cargo, "ndk", "-t", "arm64-v8a", "-P", "28", "-o", outDir.path,
            "build", "-p", "meteo", "-p", "meteo-widget-ffi") +
            (if (release) listOf("--release") else emptyList())
    )
}
tasks.named("preBuild") { dependsOn(cargoBuild) }
