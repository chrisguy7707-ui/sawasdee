#!/bin/sh
# Builds the Android app (APK). Needs: Rust + `cargo install cargo-apk` + the Android SDK/NDK + a JDK.
# Run from the app folder:  sh tools/build_android.sh
export JAVA_HOME="${JAVA_HOME:-$HOME/android-dev/jdk/jdk-17.0.21+9/Contents/Home}"
export ANDROID_HOME="${ANDROID_HOME:-$HOME/android-dev/sdk}"
export ANDROID_SDK_ROOT="$ANDROID_HOME"
export ANDROID_NDK_ROOT="${ANDROID_NDK_ROOT:-$ANDROID_HOME/ndk/26.1.10909125}"
export PATH="$HOME/.cargo/bin:$JAVA_HOME/bin:$PATH"
cargo apk build --release --lib
echo "APK: target/release/apk/Sawasdee.apk"
