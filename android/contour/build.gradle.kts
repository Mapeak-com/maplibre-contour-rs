import org.jetbrains.kotlin.gradle.tasks.KotlinCompile

plugins {
    id("com.android.library")
    id("org.jetbrains.kotlin.android")
    id("maven-publish")
}

val ndkVer = "28.2.13676358"
val abis = listOf("arm64-v8a", "armeabi-v7a", "x86_64")
// The Rust crate lives at the repo root (two levels up from this module).
val coreDir = file("../..")
val libName = "libmaplibre_contour_rs.so"

android {
    namespace = "com.mapeak.maplibrecontour"
    compileSdk = 34
    ndkVersion = ndkVer

    defaultConfig {
        minSdk = 21
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions {
        jvmTarget = "17"
    }

    // Rust .so files (built by :cargoNdkBuild) + generated UniFFI Kotlin.
    sourceSets["main"].jniLibs.srcDir(layout.buildDirectory.dir("jniLibs"))
    sourceSets["main"].java.srcDir(layout.buildDirectory.dir("generated/uniffi"))

    publishing {
        singleVariant("release") { withSourcesJar() }
    }
}

val jniLibsDir = layout.buildDirectory.dir("jniLibs")

// Cross-compile the Rust core into libmaplibre_contour_rs.so per ABI via cargo-ndk.
val cargoNdkBuild = tasks.register<Exec>("cargoNdkBuild") {
    workingDir = coreDir
    environment("ANDROID_NDK_HOME", android.sdkDirectory.resolve("ndk/$ndkVer").absolutePath)
    val cargoArgs = mutableListOf("ndk", "-o", jniLibsDir.get().asFile.absolutePath, "-P", "21")
    abis.forEach { cargoArgs += listOf("-t", it) }
    cargoArgs += listOf("build", "--release", "--features", "ffi")
    commandLine("cargo")
    setArgs(cargoArgs)
    outputs.dir(jniLibsDir)
}

// Generate the UniFFI Kotlin bindings from the compiled library's metadata.
val uniffiBindgen = tasks.register<Exec>("uniffiBindgen") {
    dependsOn(cargoNdkBuild)
    workingDir = coreDir
    val lib = jniLibsDir.get().file("arm64-v8a/$libName").asFile
    val outDir = layout.buildDirectory.dir("generated/uniffi").get().asFile
    outputs.dir(outDir)
    commandLine(
        "cargo", "run", "--quiet", "--features", "uniffi-cli", "--bin", "uniffi-bindgen", "--",
        "generate",
        "--library", lib.absolutePath,
        "--language", "kotlin",
        "--out-dir", outDir.absolutePath,
        "--no-format",
    )
}

tasks.named("preBuild").configure { dependsOn(cargoNdkBuild) }
tasks.withType<KotlinCompile>().configureEach { dependsOn(uniffiBindgen) }
// The sources jar (withSourcesJar) also reads the generated/uniffi dir.
tasks.matching { it.name.startsWith("source") && it.name.endsWith("Jar") }
    .configureEach { dependsOn(uniffiBindgen) }

dependencies {
    implementation("net.java.dev.jna:jna:5.19.0@aar")
}

// The JNA dependency must resolve as the Android `aar` (it ships
// libjnidispatch.so) — but the `@aar` type is only recorded in the POM, not in
// Gradle Module Metadata. Disable module metadata so consumers use the POM and
// get the aar; otherwise they get the desktop jar and hit UnsatisfiedLinkError.
tasks.withType<GenerateModuleMetadata>().configureEach { enabled = false }

publishing {
    publications {
        register<MavenPublication>("release") {
            groupId = "com.mapeak"
            artifactId = "contour"
            version = (System.getenv("VERSION")
                ?: System.getenv("PACKAGE_VERSION")
                ?: "0.1.0").removePrefix("v")
            afterEvaluate { from(components["release"]) }
        }
    }
}
