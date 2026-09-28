#!/usr/bin/env bash
# Build the app for the iOS Simulator (no signing needed), run it, and screenshot
# several states. Each state is set by rewriting the app's settings.json and relaunching.
set -euo pipefail

BUNDLE=io.github.divhanthelion.kjvinterlinear
OUT=screens/ios
mkdir -p "$OUT"

project=$(find app/gen/apple -maxdepth 1 -name '*.xcodeproj' | head -1)
scheme=$(xcodebuild -list -project "$project" -json | python3 -c "import json,sys; print([s for s in json.load(sys.stdin)['project']['schemes'] if s.endswith('_iOS')][0])")
echo "Project $project, scheme $scheme"

xcodebuild -project "$project" -scheme "$scheme" -configuration debug \
  -sdk iphonesimulator -destination 'generic/platform=iOS Simulator' -arch arm64 \
  -derivedDataPath build/ios CODE_SIGNING_ALLOWED=NO build | tail -40
app=$(find build/ios/Build/Products -name '*.app' -maxdepth 2 | head -1)
echo "Built $app"

device=$(xcrun simctl list devices available -j | python3 -c "
import json,sys
d=json.load(sys.stdin)['devices']
phones=[x for rt,xs in d.items() if 'iOS' in rt for x in xs if x['name'].startswith('iPhone') and 'Pro Max' in x['name']]
print(phones[-1]['udid'])")
xcrun simctl boot "$device"
xcrun simctl bootstatus "$device" -b
xcrun simctl install "$device" "$app"

launch() {
  xcrun simctl terminate "$device" "$BUNDLE" 2>/dev/null || true
  xcrun simctl launch "$device" "$BUNDLE"
  sleep "${1:-8}"
}

launch 25
xcrun simctl io "$device" screenshot "$OUT/01-first-launch.png"
data=$(xcrun simctl get_app_container "$device" "$BUNDLE" data)
settings=$(find "$data" -name settings.json | head -1)
echo "Settings file: ${settings:-not found}"
[ -n "$settings" ] || { find "$data" -maxdepth 4; exit 1; }

shot() { # name json
  echo "$2" > "$settings"
  launch 8
  xcrun simctl io "$device" screenshot "$OUT/$1.png"
}

base='"textScale":1,"textFont":"serif","verseNumbers":true,"redLetter":true,"translit":true,"strongs":true,"morph":false,"origScale":1.3,"bookmarks":[],"history":[]'
shot 02-interlinear-genesis1-dark "{\"theme\":\"dark\",\"view\":\"interlinear\",\"position\":{\"book\":\"Genesis\",\"chapter\":1,\"verse\":1},$base}"
shot 03-parallel-psalm23-light   "{\"theme\":\"light\",\"view\":\"parallel\",\"position\":{\"book\":\"Psalms\",\"chapter\":23,\"verse\":1},$base}"
shot 04-original-matthew5-dark   "{\"theme\":\"dark\",\"view\":\"original\",\"position\":{\"book\":\"Matthew\",\"chapter\":5,\"verse\":1},$base}"
shot 05-kjv-john11-redletter     "{\"theme\":\"light\",\"view\":\"kjv\",\"position\":{\"book\":\"John\",\"chapter\":11,\"verse\":35},$base}"
shot 06-interlinear-john1-light  "{\"theme\":\"light\",\"view\":\"interlinear\",\"position\":{\"book\":\"John\",\"chapter\":1,\"verse\":1},$base}"
ls -la "$OUT"
