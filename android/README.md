# redbx for Android

An encrypted, embedded key-value database for Android. The storage engine is
[redbx](https://redbx.inoerawan.id) (Rust); this module is the Android library
that wraps it.

```
crates/redbx-mobile/        Rust FFI surface (UniFFI)
crates/uniffi-bindgen/      binding generator binary
scripts/build_mobile.sh     cross-compiles the .so files, generates Kotlin bindings
android/redbx/              this Gradle library module
```

Everything public lives in one package, `id.inoerawan.redbx`, but comes from two
places:

| Source directory      | What it is                                          |
| --------------------- | --------------------------------------------------- |
| `src/main/kotlin`     | hand-written, idiomatic Kotlin API — use this        |
| `src/generated/kotlin` | UniFFI-generated bindings — regenerated, never edited |

They share a package deliberately. A separate sub-package would need a
`typealias` to re-export `RedbxValue`, and Kotlin typealiases cannot reach nested
classifiers — `RedbxValue.Str` would not resolve for callers.

## Building

### One-time setup

1. **Rust targets and cargo-ndk**

   ```bash
   rustup target add aarch64-linux-android armv7-linux-androideabi \
                     x86_64-linux-android i686-linux-android
   cargo install cargo-ndk
   ```

2. **JDK 17 or 21.** Gradle 8.11 / AGP 8.7 do not support newer JDKs, so a newer
   system JDK has to be overridden for this build:

   ```bash
   export JAVA_HOME=$HOME/Applications/android-studio/jbr   # the JDK 21 Android Studio ships
   ```

3. **Android SDK + NDK r27 or newer.** NDK r28 defaults to 16 KB page alignment,
   which Android 15 requires; on r27 the build script passes the linker flag
   explicitly.

   ```bash
   export ANDROID_HOME=$HOME/Android/Sdk
   export ANDROID_NDK_HOME=$ANDROID_HOME/ndk/<version>
   ```

Gradle itself does not need installing — the wrapper (`android/gradlew`) is
checked in and fetches its own distribution on first run.

Verify the whole toolchain at any time:

```bash
just check_deps          # or: bash scripts/check_mobile_deps.sh android
```

### Build

```bash
just build_android       # .so for 4 ABIs + Kotlin bindings
just aar_android         # the above, then the release AAR
just check_android       # compiles the instrumentation tests, no device needed
just test_android        # runs them — needs a device or emulator
```

Outputs (all gitignored — regenerate, never commit):

```
android/redbx/src/main/jniLibs/<abi>/libredbx_mobile.so
android/redbx/src/generated/kotlin/id/inoerawan/redbx/redbx_mobile.kt
android/redbx/build/outputs/aar/redbx-release.aar
```

Changing anything in `crates/redbx-mobile/` means re-running `just build_android`
before Gradle — the bindings are generated from the compiled library, so a stale
`.so` yields stale Kotlin.

## Usage

```kotlin
import id.inoerawan.redbx.*

val db = Redbx.open(context.getDatabasePath("app.redbx"), password)

db.write { txn ->
    val settings = txn.table("settings")
    settings.insert("theme".toRedbx(), "dark".toRedbx())
    settings.insert("launches".toRedbx(), 42L.toRedbx())
}

val theme = db.read { txn ->
    txn.table("settings").get("theme".toRedbx())?.asStringOrNull()
}

db.close()
```

`Redbx.open`, `write`, `read` and `compact` are `suspend` and run on
`Dispatchers.IO`. Each has a `*Blocking` twin for callers already off the main
thread — calling one on the main thread will ANR.

### Transactions

`write` commits when the block returns and aborts if it throws:

```kotlin
db.write { txn ->
    txn.table("accounts").insert(from.toRedbx(), newFromBalance.toRedbx())
    check(newToBalance >= 0) { "overdraft" }   // aborts, nothing is written
    txn.table("accounts").insert(to.toRedbx(), newToBalance.toRedbx())
}
```

Only one write transaction can exist at a time. `write` always releases it, so a
failed block cannot wedge later writes. Readers are not blocked by a writer.

Do not let a `Table` escape the block. Its handle is released when the block ends,
so using it afterwards throws `IllegalStateException` ("… object has already been
destroyed") — the call fails at the FFI boundary rather than reaching the database.

### Key ordering

`RedbxValue` variants occupy disjoint key ranges, so a range query must use the
same variant at both ends:

```kotlin
table.range(1L.toRedbx(), 100L.toRedbx())    // ok
table.range(1.toRedbx(), 100L.toRedbx())     // throws RedbxException.InvalidRange
```

Within a variant, ordering is the natural one: `-5 < 0 < 5`, `1 < 2 < 10 < 100`,
and floats follow IEEE-754 `totalOrder`. Choose the variant to match how you will
query — a number stored as `Str` sorts lexicographically, so `"10"` comes before
`"9"`.

`range()` materialises the whole result in memory. Bound it on large tables.

### Errors

Everything throws a subclass of `RedbxException`:
`IncorrectPassword`, `TableDoesNotExist`, `TransactionConsumed`, `InvalidRange`,
`DatabaseCorrupted`, `IoException`, and others.

Variants that carry diagnostic text expose it as `detail`, not `message` — a
field called `message` collides with `Throwable.message` and makes UniFFI emit
bindings that do not compile. `message` is still there, inherited from
`Throwable`, and renders as `detail=…`.

```kotlin
try {
    db.read { txn -> txn.table("missing").get("k".toRedbx()) }
} catch (e: RedbxException.TableDoesNotExist) {
    Log.w("redbx", "no such table: ${e.detail}")
}
```

### Passwords

`password` is a `String`, so it stays in the JVM heap until GC and cannot be
zeroed. If your threat model cares, keep the window short and consider fronting
it with the Android Keystore. Changing the FFI signature to `ByteArray` is
tracked as follow-up work.

## Consuming the AAR

`assembleRelease` produces `android/redbx/build/outputs/aar/redbx-release.aar`.
To publish to a local Maven repository:

```bash
cd android && ./gradlew :redbx:publishReleasePublicationToMavenLocal
```

Then depend on `id.inoerawan:redbx-android:0.1.0`. Override the version with
`-PredbxVersion=1.2.3`.

The module ships consumer ProGuard rules that keep JNA and the generated
bindings; no R8 configuration is needed downstream.

## Supported configuration

| | |
| --- | --- |
| ABIs | `arm64-v8a`, `armeabi-v7a`, `x86_64`, `x86` |
| minSdk | 24 (must match `ANDROID_MIN_SDK` in `scripts/build_mobile.sh`) |
| compileSdk | 35 |
| 16 KB pages | yes — NDK r28, or r27 with the linker flag the script sets |

## Testing

| Layer | Command | Covers |
| --- | --- | --- |
| Rust unit + integration | `just test_mobile` | encoding, ordering, transaction semantics — on the host |
| Android instrumentation | `just test_android` | the real `.so`, JNA loading, packaging, per-ABI behaviour |

The host tests cannot catch packaging or ABI problems, so a change is not
verified for Android until the instrumentation tests have run on a device.

## iOS

iOS support was removed from this branch: the Linux cross-compilation route
depended on `xtool`, which requires extracting the iOS SDK from Xcode, and that
conflicts with Apple's licence terms. iOS will be built on a macOS host instead.
`scripts/build_mobile.sh` still rejects an `ios` argument with a pointer to this.
