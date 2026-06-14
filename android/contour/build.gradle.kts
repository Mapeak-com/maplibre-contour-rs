import org.gradle.api.DefaultTask
import org.gradle.api.file.DirectoryProperty
import org.gradle.api.tasks.InputDirectory
import org.gradle.api.tasks.Internal
import org.gradle.api.tasks.OutputDirectory
import org.gradle.api.tasks.PathSensitive
import org.gradle.api.tasks.PathSensitivity
import org.gradle.api.tasks.TaskAction
import org.gradle.process.ExecOperations
import org.jetbrains.kotlin.gradle.dsl.JvmTarget
import javax.inject.Inject

plugins {
    id("com.android.library")
    id("net.mullvad.rust-android") version "0.10.1"
    id("maven-publish")
}

val ndkVer = "28.2.13676358"

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

    publishing {
        singleVariant("release") { withSourcesJar() }
    }
}

kotlin {
    compilerOptions {
        jvmTarget.set(JvmTarget.JVM_17)
    }
}

// The plugin cross-compiles the root crate into build/rustJniLibs/android/<abi>/
// and adds them to jniLibs. (libname "maplibre_contour_rs" ->
// libmaplibre_contour_rs.so per ABI.)
cargo {
    module = "../.."
    libname = "maplibre_contour_rs"
    targets = listOf("arm64", "arm", "x86_64")
    profile = "release"
    apiLevel = 21
    pythonCommand = "python3"
}

// Generates the UniFFI Kotlin bindings from the compiled library's metadata.
abstract class UniffiBindgen @Inject constructor(private val execOps: ExecOperations) : DefaultTask() {
    @get:OutputDirectory abstract val outputDir: DirectoryProperty
    @get:InputDirectory @get:PathSensitive(PathSensitivity.RELATIVE) abstract val rustJniLibs: DirectoryProperty
    @get:Internal abstract val coreDir: DirectoryProperty

    @TaskAction
    fun generate() {
        val out = outputDir.get().asFile.apply { mkdirs() }
        val lib = rustJniLibs.get().dir("arm64-v8a").file("libmaplibre_contour_rs.so").asFile
        execOps.exec {
            workingDir = coreDir.get().asFile
            commandLine = listOf(
                "cargo", "run", "--quiet", "--bin", "uniffi-bindgen", "--",
                "generate", "--library", lib.absolutePath,
                "--language", "kotlin", "--out-dir", out.absolutePath, "--no-format",
            )
        }
    }
}

val rustJniLibsDir = layout.buildDirectory.dir("rustJniLibs/android")

androidComponents {
    onVariants(selector().withBuildType("release")) { variant ->
        val bindgen = tasks.register<UniffiBindgen>("uniffiBindgen") {
            dependsOn("cargoBuild")
            coreDir.set(layout.projectDirectory.dir("../.."))
            rustJniLibs.set(rustJniLibsDir)
        }
        variant.sources.java?.addGeneratedSourceDirectory(bindgen, UniffiBindgen::outputDir)
    }
}

// Make the JNI merge step wait for the cargo build (per the plugin's README).
tasks.matching { it.name.matches(Regex("merge.*JniLibFolders")) }.configureEach {
    inputs.dir(rustJniLibsDir)
    dependsOn("cargoBuild")
}

dependencies {
    implementation("net.java.dev.jna:jna:5.19.1@aar")
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
