#!/usr/bin/env bash
# Usage: scripts/install.sh <device-name|mac>
# Builds pkm (Release) and installs it on a connected device, or into
# /Applications for "mac".
#
# The bundle ID defaults to com.timhilt.pkm; BUNDLE_ID overrides it.
set -euo pipefail

config=Release
target="${1:?usage: $0 <device-name|mac>}"
root="$(cd "$(dirname "$0")/.." && pwd)"
build="$root/apple/build"
app=pkm
product="pkm.app"

bundle_id="${BUNDLE_ID:-com.timhilt.pkm}"
# Apple registers an App ID on the first device build under a new bundle ID and
# caps that at 10 per team every 7 days. When the cap is hit, the build fails
# with "maximum App ID limit" until the window rolls over, so retry under an ID
# that is already registered for this app: com.timhilt.workingtitle is its
# pre-rebrand ID (see "Rebrand WORKING-TITLE to pkm"). Installed
# that way, the app keeps the container of that ID, not the canonical one.
fallback_bundle_ids=(com.timhilt.workingtitle)

cd "$root"
make apple-project

cd apple
if [[ "$target" == mac ]]; then
  xcodebuild build -quiet -project $app.xcodeproj -scheme $app -configuration "$config" \
    -destination 'platform=macOS' -derivedDataPath "$build" -allowProvisioningUpdates \
    APP_BUNDLE_ID="$bundle_id"
  rm -rf /Applications/$app.app "/Applications/$product"
  cp -R "$build/Build/Products/$config/$product" /Applications/
  open "/Applications/$product"
else
  candidates=("$bundle_id")
  # Respect an explicit BUNDLE_ID: only fall back when nothing was chosen.
  [[ -n "${BUNDLE_ID:-}" ]] || candidates+=("${fallback_bundle_ids[@]}")

  installed_id=""
  for id in "${candidates[@]}"; do
    log="$build/install-$id.log"
    echo "Building for $id (log: $log)" >&2
    if xcodebuild build -quiet -project $app.xcodeproj -scheme $app -configuration "$config" \
      -destination "platform=iOS,name=$target" -derivedDataPath "$build" -allowProvisioningUpdates \
      APP_BUNDLE_ID="$id" 2>&1 | tee "$log"; then
      installed_id=$id
      break
    fi
    # Only Apple's App ID quota is worth retrying under another ID; anything
    # else is a real build failure.
    if grep -q 'No Accounts' "$log"; then
      echo "error: Xcode is not signed in; add your Apple ID in Xcode > Settings > Accounts and retry" >&2
      exit 1
    fi
    grep -q 'maximum App ID limit' "$log" || exit 1
    echo "Apple refuses new App IDs this week; trying an ID that is already registered." >&2
  done
  if [[ -z "$installed_id" ]]; then
    echo "error: no registered App ID to install under; wait for Apple's quota to roll over or pass BUNDLE_ID=<id>" >&2
    exit 1
  fi
  [[ "$installed_id" == "$bundle_id" ]] \
    || echo "note: installing as $installed_id; this app's Drawings stay in that ID's container" >&2

  xcrun devicectl device install app --device "$target" \
    "$build/Build/Products/$config-iphoneos/$product"
  xcrun devicectl device process launch --device "$target" "$installed_id"
fi
