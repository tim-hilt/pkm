#!/usr/bin/env bash
# Usage: scripts/install.sh <device-name|mac>
# Builds Excalidraw Native (Release) and installs it on a connected device, or into
# /Applications for "mac".
set -euo pipefail

config=Release
target="${1:?usage: $0 <device-name|mac>}"
root="$(cd "$(dirname "$0")/.." && pwd)"
build="$root/apple/build"
bundle_id=com.timhilt.excalidrawnative
app=ExcalidrawNative

cd "$root"
make apple-project

cd apple
if [[ "$target" == mac ]]; then
  xcodebuild build -quiet -project $app.xcodeproj -scheme $app -configuration "$config" \
    -destination 'platform=macOS' -derivedDataPath "$build" -allowProvisioningUpdates
  rm -rf /Applications/$app.app
  cp -R "$build/Build/Products/$config/$app.app" /Applications/
  open /Applications/$app.app
else
  xcodebuild build -quiet -project $app.xcodeproj -scheme $app -configuration "$config" \
    -destination "platform=iOS,name=$target" -derivedDataPath "$build" -allowProvisioningUpdates
  xcrun devicectl device install app --device "$target" \
    "$build/Build/Products/$config-iphoneos/$app.app"
  xcrun devicectl device process launch --device "$target" "$bundle_id"
fi
