#!/bin/sh
# Xcode runs this before compiling: it builds the Rust library for the chosen device type.
set -e
export PATH="$HOME/.cargo/bin:$PATH"
cd "$SRCROOT/.."
case "$PLATFORM_NAME" in
  iphonesimulator) TARGET=aarch64-apple-ios-sim ;;
  iphoneos) TARGET=aarch64-apple-ios ;;
  *) echo "unsupported platform $PLATFORM_NAME"; exit 1 ;;
esac
# Xcode's own settings must not leak into cargo's build of the Rust tools.
unset SDKROOT LD LDPLUSPLUS CC CXX
cargo rustc --lib --release --target "$TARGET" --crate-type staticlib
