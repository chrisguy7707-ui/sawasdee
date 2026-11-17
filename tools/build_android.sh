#!/bin/sh
# Builds the Android app (APK). Needs: Rust + `cargo install cargo-apk` + the Android SDK/NDK + a JDK.
# Run from the app folder:  sh tools/build_android.sh
export JAVA_HOME="${JAVA_HOME:-$HOME/android-dev/jdk/jdk-17.0.21+9/Contents/Home}"
export ANDROID_HOME="${ANDROID_HOME:-$HOME/android-dev/sdk}"
export ANDROID_SDK_ROOT="$ANDROID_HOME"
export ANDROID_NDK_ROOT="${ANDROID_NDK_ROOT:-$ANDROID_HOME/ndk/26.1.10909125}"
export PATH="$HOME/.cargo/bin:$JAVA_HOME/bin:$PATH"
# Release signing key. The password is read from android/keystore.password (never committed)
# or from CARGO_APK_RELEASE_KEYSTORE_PASSWORD.
APP_DIR="$(cd "$(dirname "$0")/.." && pwd)"
export CARGO_APK_RELEASE_KEYSTORE="${CARGO_APK_RELEASE_KEYSTORE:-$APP_DIR/android/release.keystore}"
if [ -z "$CARGO_APK_RELEASE_KEYSTORE_PASSWORD" ] && [ -f "$APP_DIR/android/keystore.password" ]; then
  CARGO_APK_RELEASE_KEYSTORE_PASSWORD="$(cat "$APP_DIR/android/keystore.password")"
  export CARGO_APK_RELEASE_KEYSTORE_PASSWORD
fi
if [ ! -f "$CARGO_APK_RELEASE_KEYSTORE" ] || [ -z "$CARGO_APK_RELEASE_KEYSTORE_PASSWORD" ]; then
  echo "Missing signing key: need android/release.keystore and android/keystore.password" >&2
  exit 1
fi
# 16 KB memory pages: Google Play requires native libraries to support them (Android 15+ devices).
export RUSTFLAGS="${RUSTFLAGS:+$RUSTFLAGS }-C link-arg=-Wl,-z,max-page-size=16384"
# "sh tools/build_android.sh play" = Google Play build: units after the free ones need the one-time purchase.
# Without it (beta APK for friends, sideloaded) every unit is open, because purchases only work through Google Play.
FEATURES=""
if [ "$1" = "play" ]; then FEATURES="--features play"; fi
cargo apk build --release --lib $FEATURES
# cargo-apk made the native library; tools/android_pack.py builds the final APK around it
# (own manifest, reminder Java code, alignment, signing).
python3 "$APP_DIR/tools/android_pack.py"
if [ "$1" = "play" ]; then
  mv target/release/apk/Sawasdee.apk target/release/apk/Sawasdee-play.apk
  mv target/release/apk/Sawasdee.aab target/release/apk/Sawasdee-play.aab
  echo "Play build: target/release/apk/Sawasdee-play.aab (upload this) and Sawasdee-play.apk (for testing)"
else
  echo "APK: target/release/apk/Sawasdee.apk"
fi
