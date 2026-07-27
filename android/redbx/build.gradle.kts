plugins {
    id("com.android.library")
    id("org.jetbrains.kotlin.android")
    id("maven-publish")
}

android {
    namespace = "id.inoerawan.redbx"
    compileSdk = 35

    defaultConfig {
        // Keep in sync with ANDROID_MIN_SDK in scripts/build_mobile.sh — the .so
        // files are compiled against that API level.
        minSdk = 24
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
        consumerProguardFiles("consumer-rules.pro")

        ndk {
            abiFilters += listOf("arm64-v8a", "armeabi-v7a", "x86_64", "x86")
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    sourceSets.getByName("main") {
        // UniFFI-generated bindings, produced by scripts/build_mobile.sh android.
        // Kept out of src/main/kotlin so generated and hand-written code stay
        // visibly separate (and so the generated tree can be gitignored wholesale).
        java.srcDir("src/generated/kotlin")
    }

    packaging {
        jniLibs {
            // Required for 16 KB page size support on Android 15+: the .so files
            // must stay uncompressed and page-aligned in the APK.
            useLegacyPackaging = false
        }
    }

    publishing {
        singleVariant("release") {
            withSourcesJar()
        }
    }
}

kotlin {
    compilerOptions {
        // Target Java 17 bytecode, but compile with whatever JDK Gradle runs on
        // (17 or 21). Using `jvmToolchain(17)` instead would hard-require a JDK 17
        // installation and fail on a machine that only has 21.
        jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17)
    }
}

dependencies {
    // UniFFI's Kotlin bindings call into the .so through JNA. The `@aar` classifier
    // is mandatory — the plain JAR does not ship the Android native dispatch libs
    // and fails at runtime with UnsatisfiedLinkError.
    implementation("net.java.dev.jna:jna:5.15.0@aar")

    // `api`, not `implementation`: the public API exposes suspend functions, so
    // consumers compile against coroutines.
    api("org.jetbrains.kotlinx:kotlinx-coroutines-core:1.9.0")

    androidTestImplementation("androidx.test.ext:junit:1.2.1")
    androidTestImplementation("androidx.test:runner:1.6.2")
    androidTestImplementation("org.jetbrains.kotlinx:kotlinx-coroutines-test:1.9.0")
}

afterEvaluate {
    publishing {
        publications {
            create<MavenPublication>("release") {
                from(components["release"])
                groupId = "id.inoerawan"
                artifactId = "redbx-android"
                version = project.findProperty("redbxVersion")?.toString() ?: "0.1.0"

                pom {
                    name.set("redbx-android")
                    description.set("Encrypted embedded key-value database for Android, backed by redbx")
                    url.set("https://redbx.inoerawan.id")
                    licenses {
                        license {
                            name.set("MIT")
                            url.set("https://opensource.org/licenses/MIT")
                        }
                        license {
                            name.set("Apache-2.0")
                            url.set("https://www.apache.org/licenses/LICENSE-2.0")
                        }
                    }
                }
            }
        }
    }
}
