# Consumers of redbx-android inherit these rules.
#
# JNA resolves types and dispatches native calls reflectively, and the
# UniFFI-generated bindings define JNA Structure/Callback subclasses that R8 has
# no static reference to. Stripping or renaming either breaks at runtime, not at
# build time — hence the broad keeps.

-keep class com.sun.jna.** { *; }
-keepclassmembers class * extends com.sun.jna.** { public *; }
-keep interface com.sun.jna.** { *; }

# UniFFI-generated bindings: structures, callbacks and the library interface are
# all reached from native code. The generated sources share the public package,
# so this necessarily covers the hand-written wrapper too — acceptable for a
# library this size.
-keep class id.inoerawan.redbx.** { *; }
-keep interface id.inoerawan.redbx.** { *; }
