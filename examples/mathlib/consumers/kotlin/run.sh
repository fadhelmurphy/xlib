#!/usr/bin/env bash
# Compile Main.kt + binding hasil `xlib build`, lalu jalankan di JVM.
set -e
cd "$(dirname "$0")"
DIST=../../dist/kotlin
JNA=${JNA_JAR:-/usr/share/java/jna.jar}
kotlinc -nowarn Main.kt "$DIST/uniffi/mathlib/mathlib.kt" -cp "$JNA" -include-runtime -d app.jar 2>/dev/null
java -Djna.library.path="$DIST" -cp "app.jar:$JNA" MainKt
