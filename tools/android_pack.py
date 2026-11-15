#!/usr/bin/env python3
"""Packages the Android app with our own manifest and Java code, then aligns and signs it.

`cargo apk` cannot add Java classes (needed for the daily reminder alarms) or extra manifest
entries (receivers), so tools/build_android.sh first lets cargo-apk build the native library,
then this script builds the final APK from:
    android/pkg/AndroidManifest.xml      hand-written manifest
    android/res/                         icons and the notification icon
    android/java/**/*.java               reminder code, compiled to classes.dex
    target/aarch64-linux-android/release/libsawasdee.so
Signing uses android/release.keystore and android/keystore.password (never committed).
Output: target/release/apk/Sawasdee.apk   (replaces the cargo-apk file)
"""
import glob
import os
import re
import shutil
import subprocess
import sys
import tempfile
import zipfile

HERE = os.path.dirname(os.path.abspath(__file__))
APP = os.path.dirname(HERE)
SDK = os.environ.get("ANDROID_HOME", os.path.expanduser("~/android-dev/sdk"))
JAVA_HOME = os.environ.get("JAVA_HOME", os.path.expanduser("~/android-dev/jdk/jdk-17.0.21+9/Contents/Home"))
PLATFORM = "36"
MIN_SDK, TARGET_SDK = "26", "36"


def run(cmd, **kw):
    r = subprocess.run(cmd, capture_output=True, text=True, **kw)
    if r.returncode != 0:
        sys.exit(f"command failed: {' '.join(cmd)}\n{r.stdout}\n{r.stderr}")
    return r


def main():
    # d8 and apksigner are scripts that need a Java runtime
    os.environ["JAVA_HOME"] = JAVA_HOME
    os.environ["PATH"] = os.path.join(JAVA_HOME, "bin") + os.pathsep + os.environ.get("PATH", "")
    bt = sorted(glob.glob(os.path.join(SDK, "build-tools", PLATFORM + "*")))
    if not bt:
        sys.exit("Android build-tools 36 not installed")
    bt = bt[-1]
    aapt2, zipalign, apksigner, d8 = (os.path.join(bt, n) for n in ("aapt2", "zipalign", "apksigner", "d8"))
    android_jar = os.path.join(SDK, "platforms", f"android-{PLATFORM}", "android.jar")
    javac = os.path.join(JAVA_HOME, "bin", "javac")
    so = os.path.join(APP, "target", "aarch64-linux-android", "release", "libsawasdee.so")
    if not os.path.exists(so):
        sys.exit(f"{so} not found: run cargo apk build first (tools/build_android.sh does this)")

    cargo = open(os.path.join(APP, "Cargo.toml"), encoding="utf-8").read()
    version = re.search(r'^version = "([^"]+)"', cargo, re.M).group(1)
    ma, mi, pa = (int(x) for x in version.split("."))
    version_code = 0x1000000 + (ma << 16) + (mi << 8) + pa  # 0.3.0 -> 16777984, same as before

    ks = os.environ.get("CARGO_APK_RELEASE_KEYSTORE", os.path.join(APP, "android", "release.keystore"))
    pw = os.environ.get("CARGO_APK_RELEASE_KEYSTORE_PASSWORD")
    pwfile = os.path.join(APP, "android", "keystore.password")
    if not pw and os.path.exists(pwfile):
        pw = open(pwfile).read().strip()
    if not (os.path.exists(ks) and pw):
        sys.exit("Missing signing key: need android/release.keystore and android/keystore.password")

    out_dir = os.path.join(APP, "target", "release", "apk")
    os.makedirs(out_dir, exist_ok=True)
    final = os.path.join(out_dir, "Sawasdee.apk")
    with tempfile.TemporaryDirectory() as tmp:
        # 1. resources + manifest
        res_zip = os.path.join(tmp, "res.zip")
        run([aapt2, "compile", "--dir", os.path.join(APP, "android", "res"), "-o", res_zip])
        base = os.path.join(tmp, "base.apk")
        run([aapt2, "link", "-o", base, "-I", android_jar,
             "--manifest", os.path.join(APP, "android", "pkg", "AndroidManifest.xml"),
             "-R", res_zip, "--min-sdk-version", MIN_SDK, "--target-sdk-version", TARGET_SDK,
             "--version-code", str(version_code), "--version-name", version,
             "--compile-sdk-version-code", PLATFORM, "--compile-sdk-version-name", "16"])
        # 2. Java -> classes.dex
        classes = os.path.join(tmp, "classes")
        os.makedirs(classes)
        sources = glob.glob(os.path.join(APP, "android", "java", "**", "*.java"), recursive=True)
        run([javac, "-source", "8", "-target", "8", "-bootclasspath", android_jar, "-Xlint:-options", "-d", classes] + sources)
        class_files = glob.glob(os.path.join(classes, "**", "*.class"), recursive=True)
        dex_dir = os.path.join(tmp, "dex")
        os.makedirs(dex_dir)
        run([d8, "--lib", android_jar, "--release", "--min-api", MIN_SDK, "--output", dex_dir] + class_files)
        # 3. assemble: base entries + classes.dex + native library (uncompressed, so it can be page-aligned)
        unsigned = os.path.join(tmp, "unsigned.apk")
        with zipfile.ZipFile(base) as zin, zipfile.ZipFile(unsigned, "w") as zout:
            for info in zin.infolist():
                zout.writestr(info, zin.read(info.filename), compress_type=info.compress_type)
            zout.write(os.path.join(dex_dir, "classes.dex"), "classes.dex", compress_type=zipfile.ZIP_DEFLATED)
            zout.write(so, "lib/arm64-v8a/libsawasdee.so", compress_type=zipfile.ZIP_STORED)
        # 4. align (16 KB pages, which Google Play requires) and sign
        aligned = os.path.join(tmp, "aligned.apk")
        run([zipalign, "-f", "-P", "16", "4", unsigned, aligned])
        if os.path.exists(final):
            os.remove(final)
        run([apksigner, "sign", "--ks", ks, "--ks-pass", "pass:" + pw, "--out", final, aligned])
        r = run([apksigner, "verify", "--print-certs", final])
        cert = [l for l in r.stdout.splitlines() if "SHA-256" in l]
        print(f"APK: {final}  ({os.path.getsize(final) / 1e6:.1f} MB, version {version}, code {version_code})")
        print(cert[0].strip() if cert else "signed")


main()
