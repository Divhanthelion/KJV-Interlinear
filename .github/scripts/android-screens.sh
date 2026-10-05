#!/usr/bin/env bash
# Install the debug APK on the running emulator and screenshot several app states.
# Each state is set by rewriting the app's settings.json and relaunching.
#
# Readiness: the page logs "kjv:ready <book> <chapter>" once the chapter is in the
# DOM, which reaches logcat as a chromium CONSOLE line. uiautomator can't see
# WebView text (and the status-bar clock defeats any digit grep), so that console
# marker is what we wait for — and a state that never becomes ready FAILS the job.
set -euo pipefail

PKG=io.github.divhanthelion.kjvinterlinear
OUT=screens/android
mkdir -p "$OUT"

apk=$(find app/gen/android/app/build/outputs/apk -name '*.apk' | head -1)
echo "Installing $apk"
adb install -r "$apk"
adb shell settings put global window_animation_scale 0
adb shell settings put global transition_animation_scale 0
adb shell settings put global animator_duration_scale 0

diagnose() {
  echo "--- logcat (app, webview, crashes) ---"
  adb logcat -d | grep -iE "CONSOLE|RustStdoutStderr|tauri|FATAL|AndroidRuntime|backtrace" | tail -60 || true
}

# Wait (up to 90 s) for the page's kjv:ready console line, then let paint settle
wait_ready() {
  for i in $(seq 45); do
    if adb logcat -d | grep -q "kjv:ready"; then
      sleep 2
      return 0
    fi
    sleep 2
  done
  echo "the app never logged kjv:ready"
  diagnose
  return 1
}

launch() {
  adb shell am force-stop "$PKG"
  adb logcat -c || true
  adb shell monkey -p "$PKG" -c android.intent.category.LAUNCHER 1 > /dev/null
  wait_ready
  # A slow emulator can raise "isn't responding" dialogs for other apps
  adb shell am broadcast -a android.intent.action.CLOSE_SYSTEM_DIALOGS > /dev/null 2>&1 || true
}

# First launch: the app saves its settings once the first chapter loads
launch
adb exec-out screencap -p > "$OUT/01-first-launch.png"
settings=$(adb shell run-as "$PKG" find . -name settings.json | tr -d '\r' | head -1)
echo "Settings file: ${settings:-not found}"
if [ -z "$settings" ]; then
  diagnose
  exit 1
fi

shot() { # name json
  # Stop the app first so it can't overwrite the state we are about to write
  adb shell am force-stop "$PKG"
  # One quoted string, so the redirect runs inside run-as (as the app, in its data dir)
  echo "$2" | adb shell "run-as $PKG sh -c 'cat > $settings'"
  adb shell "run-as $PKG cat $settings" | grep -q "\"view\"" || { echo "settings write failed"; exit 1; }
  launch
  adb exec-out screencap -p > "$OUT/$1.png"
}

base='"textScale":1,"textFont":"serif","verseNumbers":true,"redLetter":true,"translit":true,"strongs":true,"morph":false,"origScale":1.3,"bookmarks":[],"history":[]'
shot 02-interlinear-genesis1-dark "{\"theme\":\"dark\",\"view\":\"interlinear\",\"position\":{\"book\":\"Genesis\",\"chapter\":1,\"verse\":1},$base}"
shot 03-parallel-psalm23-light   "{\"theme\":\"light\",\"view\":\"parallel\",\"position\":{\"book\":\"Psalms\",\"chapter\":23,\"verse\":1},$base}"
shot 04-original-matthew5-dark   "{\"theme\":\"dark\",\"view\":\"original\",\"position\":{\"book\":\"Matthew\",\"chapter\":5,\"verse\":1},$base}"
shot 05-kjv-john11-redletter     "{\"theme\":\"light\",\"view\":\"kjv\",\"position\":{\"book\":\"John\",\"chapter\":11,\"verse\":35},$base}"
shot 06-interlinear-john1-light  "{\"theme\":\"light\",\"view\":\"interlinear\",\"position\":{\"book\":\"John\",\"chapter\":1,\"verse\":1},$base}"

adb logcat -d | grep -iE "CONSOLE|RustStdoutStderr|tauri|FATAL|AndroidRuntime" | tail -100 > "$OUT/logcat.txt" || true
ls -la "$OUT"
