import java.util.Properties

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

// The Wear OS app: a tile with the weather, from the same Rust core as the
// phone's widgets (libmeteo_widget.so). Same application ID as the phone
// app, as Wear OS expects for an app with a phone companion.
android {
    namespace = "dev.meteo.app"
    compileSdk = 36
    ndkVersion = "30.0.16248370"

    defaultConfig {
        applicationId = "dev.meteo.app"
        minSdk = 30
        targetSdk = 36
        versionCode = 1
        versionName = "0.1.0"
        // Many watches run 32-bit Android.
        ndk { abiFilters += listOf("arm64-v8a", "armeabi-v7a") }
    }

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

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions { jvmTarget = "17" }

    sourceSets["main"].jniLibs.srcDir(layout.buildDirectory.dir("rustJniLibs"))
    sourceSets["main"].java.srcDir("../shared/kotlin")
}

dependencies {
    implementation("androidx.wear.tiles:tiles:1.4.1")
    implementation("androidx.wear.protolayout:protolayout:1.2.1")
    implementation("androidx.concurrent:concurrent-futures:1.2.0")
}

// libmeteo_widget.so only (no UI on the watch), for both watch ABIs.
val release = gradle.startParameter.taskNames.any { it.contains("Release", ignoreCase = true) }
val cargoBuild by tasks.registering(Exec::class) {
    description = "Builds the Rust widget library with cargo-ndk."
    workingDir = rootDir.parentFile
    val outDir = layout.buildDirectory.dir("rustJniLibs").get().asFile
    environment("ANDROID_HOME", android.sdkDirectory.path)
    environment("ANDROID_NDK_HOME", android.ndkDirectory.path)
    val cargo = File(System.getProperty("user.home"), ".cargo/bin/cargo").takeIf { it.exists() }?.path ?: "cargo"
    commandLine(
        listOf(cargo, "ndk", "-t", "arm64-v8a", "-t", "armeabi-v7a", "-P", "30", "-o", outDir.path,
            "build", "-p", "meteo-widget-ffi") +
            (if (release) listOf("--release") else emptyList())
    )
}
tasks.named("preBuild") { dependsOn(cargoBuild) }
