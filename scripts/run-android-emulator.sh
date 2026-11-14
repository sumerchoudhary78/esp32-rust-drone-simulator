#!/usr/bin/env bash
set -euo pipefail
project_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
export ANDROID_HOME="${ANDROID_HOME:-$HOME/Library/Android/sdk}"
export JAVA_HOME="${JAVA_HOME:-/Applications/Android Studio.app/Contents/jbr/Contents/Home}"
export PATH="$JAVA_HOME/bin:$ANDROID_HOME/platform-tools:$PATH"
avd_name=Aerolab_Android_11
avd_manager="$ANDROID_HOME/cmdline-tools/latest/bin/avdmanager"
if [[ ! -d "$HOME/.android/avd/$avd_name.avd" ]]; then
  printf 'no\n' | "$avd_manager" create avd -n "$avd_name" -k "${AEROLAB_ANDROID_IMAGE:-system-images;android-30;default;arm64-v8a}" -d pixel_4
fi
if [[ ! -f "$project_dir/output/android/Aerolab-Controller-debug.apk" ]]; then
  bash "$project_dir/scripts/build-android.sh"
fi
"$ANDROID_HOME/emulator/emulator" -avd "$avd_name" -port 5580 -no-snapshot-save -no-audio "$@" &
emulator_pid=$!
trap 'kill "$emulator_pid" 2>/dev/null || true' EXIT
adb -s emulator-5580 wait-for-device
for _ in {1..120}; do
  if [[ "$(adb -s emulator-5580 shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" == 1 ]]; then break; fi
  sleep 1
done
adb -s emulator-5580 install -r "$project_dir/output/android/Aerolab-Controller-debug.apk"
adb -s emulator-5580 shell am start -n dev.aerolab.controller/android.app.NativeActivity
echo 'Android controller ready. Start the Rust simulator on port 8787, then tap Connect.'
wait "$emulator_pid"
