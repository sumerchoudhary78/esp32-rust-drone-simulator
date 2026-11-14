#!/usr/bin/env bash
set -euo pipefail
project_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
export ANDROID_HOME="${ANDROID_HOME:-$HOME/Library/Android/sdk}"
export ANDROID_NDK_ROOT="${ANDROID_NDK_ROOT:-$ANDROID_HOME/ndk/28.2.13676358}"
export JAVA_HOME="${JAVA_HOME:-/Applications/Android Studio.app/Contents/jbr/Contents/Home}"
export ANDROID_PLATFORM="${ANDROID_PLATFORM:-android-35}"
build_tools="${ANDROID_BUILD_TOOLS:-$ANDROID_HOME/build-tools/35.0.0}"
case "$(uname -s)" in
  Darwin) ndk_host=darwin-x86_64 ;;
  Linux) ndk_host=linux-x86_64 ;;
  *) echo "Use macOS or Linux with an Android SDK/NDK installed." >&2; exit 1 ;;
esac
ndk_bin="$ANDROID_NDK_ROOT/toolchains/llvm/prebuilt/$ndk_host/bin"
for required in "$JAVA_HOME/bin/javac" "$ndk_bin/aarch64-linux-android26-clang" "$build_tools/aapt" "$build_tools/apksigner"; do
  if [[ ! -x "$required" ]]; then echo "Missing Android build tool: $required" >&2; exit 1; fi
done
export PATH="$JAVA_HOME/bin:$ndk_bin:$PATH"
export RUSTC="$(rustup which --toolchain stable rustc)"
export CC_aarch64_linux_android="$ndk_bin/aarch64-linux-android26-clang"
export CXX_aarch64_linux_android="$ndk_bin/aarch64-linux-android26-clang++"
export AR_aarch64_linux_android="$ndk_bin/llvm-ar"
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$CC_aarch64_linux_android"
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-4}"
cargo build --manifest-path "$project_dir/apps/controller/Cargo.toml" --target aarch64-linux-android --lib
artifact_dir="$project_dir/output/android"
mkdir -p "$artifact_dir/package/lib/arm64-v8a"
cp "$project_dir/apps/controller/target/aarch64-linux-android/debug/libaerolab_controller.so" "$artifact_dir/package/lib/arm64-v8a/"
"$ndk_bin/llvm-strip" --strip-unneeded "$artifact_dir/package/lib/arm64-v8a/libaerolab_controller.so"
# Package the platform NativeActivity. Slint embeds its helper dex in the library.
"$build_tools/aapt" package -f -M "$project_dir/apps/controller/AndroidManifest.xml" -I "$ANDROID_HOME/platforms/$ANDROID_PLATFORM/android.jar" -F "$artifact_dir/unsigned.apk"
(cd "$artifact_dir/package" && zip -q -r "$artifact_dir/unsigned.apk" lib)
"$build_tools/zipalign" -P 16 -f 4 "$artifact_dir/unsigned.apk" "$artifact_dir/aligned.apk"
if [[ ! -f "$artifact_dir/debug.keystore" ]]; then
  "$JAVA_HOME/bin/keytool" -genkeypair -noprompt -keystore "$artifact_dir/debug.keystore" -storepass android -keypass android -alias androiddebugkey -keyalg RSA -keysize 2048 -validity 3650 -dname "CN=Aerolab Development,O=Local Simulator,C=IN"
fi
"$build_tools/apksigner" sign --ks "$artifact_dir/debug.keystore" --ks-key-alias androiddebugkey --ks-pass pass:android --key-pass pass:android --out "$artifact_dir/Aerolab-Controller-debug.apk" "$artifact_dir/aligned.apk"
"$build_tools/apksigner" verify "$artifact_dir/Aerolab-Controller-debug.apk"
echo "Built: $artifact_dir/Aerolab-Controller-debug.apk"
