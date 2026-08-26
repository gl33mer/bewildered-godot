#!/usr/bin/env bash
# Bewildered — headless Android APK export pipeline.
# Prerequisites (one-time):
#   - Android SDK at ~/Android/Sdk (cmdline-tools, platform-35, build-tools 35, NDK r27)
#   - Rust target: rustup target add aarch64-linux-android ; cargo install cargo-ndk
#   - Godot 4.7.2 export templates installed
#   - Debug keystore at ~/.android/debug.keystore (androiddebugkey/android)
set -euo pipefail

export ANDROID_HOME="${ANDROID_HOME:-$HOME/Android/Sdk}"
NDK="$ANDROID_HOME/ndk/27.0.12077973"
export PATH="$NDK/toolchains/llvm/prebuilt/linux-x86_64/bin:$PATH"
GODOT="${GODOT:-$HOME/.local/share/mise/installs/godot/4.7.2-stable/Godot_v4.7.2-stable_linux.x86_64}"

cd "$(dirname "$0")/.."

echo "== [1/3] Cross-compiling Rust extension (arm64-v8a) =="
(cd rust && cargo ndk -t arm64-v8a --platform 23 build)

# Strip the debug .so (cargo keeps debuginfo; ~100MB -> ~5MB).
STRIP="$NDK/toolchains/llvm/prebuilt/linux-x86_64/bin/llvm-strip"
[ -x "$STRIP" ] && "$STRIP" rust/target/aarch64-linux-android/debug/libbewildered_godot.so

echo "== [2/3] Exporting debug APK =="
mkdir -p build
"$GODOT" --headless --export-debug "Android" build/bewildered.apk

echo "== [3/3] Done =="
ls -la build/bewildered.apk
echo "Serve it:  cd build && python3 -m http.server 8080"
echo "Install:   adb install -r build/bewildered.apk"
