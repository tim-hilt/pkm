#!/usr/bin/env bash
# Usage: DEVICE=mac|ipad|iphone [SCREEN=canvas|settings] [OUT=path] scripts/screenshot.sh
# Builds the app, launches it with the sample Drawing already open
# (-UITestingOpenSampleDrawing), or with Settings open (SCREEN=settings,
# -UITestingOpenSettings) and saves a screenshot to $OUT
# (default /tmp/wt-$DEVICE.png). On Mac only the app's window is captured.
set -euo pipefail
cd "$(dirname "$0")/../apple"

DEVICE=${DEVICE:-iphone}
case "${SCREEN:-canvas}" in
canvas) LAUNCH_ARG=-UITestingOpenSampleDrawing WINDOW_NAME= ;;
settings) LAUNCH_ARG=-UITestingOpenSettings WINDOW_NAME=Settings ;;
*) echo "SCREEN must be canvas or settings" >&2; exit 2 ;;
esac
OUT=${OUT:-/tmp/wt-$DEVICE.png}
DERIVED=build
case "$DEVICE" in
mac) ;;
iphone) SIM='iPhone 15 Pro' ;;
ipad) SIM='iPad Pro (12.9-inch) (4th generation)' ;;
*) echo "DEVICE must be mac, ipad or iphone" >&2; exit 2 ;;
esac

if [ "$DEVICE" = mac ]; then
    xcodebuild -quiet -scheme ExcalidrawNative -destination 'platform=macOS' -derivedDataPath "$DERIVED" build
    app="$DERIVED/Build/Products/Debug/Excalidraw Native.app"
    pkill -x 'Excalidraw Native' 2>/dev/null || true
    open -n "$app" --args "$LAUNCH_ARG"
    sleep 8
    # Capture just the app's window, so other windows never end up in the file.
    window=$(osascript -l JavaScript -e '
        ObjC.import("CoreGraphics");
        const list = ObjC.deepUnwrap(ObjC.castRefToObject($.CGWindowListCopyWindowInfo(1, 0)));
        let id = "";
        for (const w of Array.from(list)) {
            if (w.kCGWindowOwnerName === "Excalidraw Native" && w.kCGWindowLayer === 0 && (w.kCGWindowName || "").includes("'"$WINDOW_NAME"'")) { id = w.kCGWindowNumber; break; }
        }
        id')
    screencapture -x -o -l "$window" "$OUT"
    pkill -x 'Excalidraw Native' || true
else
    xcrun simctl list devices available | grep -qF "    $SIM (" || xcrun simctl create "$SIM" "$SIM" >/dev/null
    xcodebuild -quiet -scheme ExcalidrawNative -destination "platform=iOS Simulator,name=$SIM" -derivedDataPath "$DERIVED" build
    xcrun simctl boot "$SIM" 2>/dev/null || true
    xcrun simctl bootstatus "$SIM" >/dev/null
    xcrun simctl install "$SIM" "$DERIVED/Build/Products/Debug-iphonesimulator/Excalidraw Native.app"
    xcrun simctl launch --terminate-running-process "$SIM" com.timhilt.excalidrawnative "$LAUNCH_ARG" >/dev/null
    sleep 8
    xcrun simctl io "$SIM" screenshot "$OUT"
fi
echo "$OUT"
