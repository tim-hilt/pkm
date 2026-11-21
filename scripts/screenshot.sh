#!/usr/bin/env bash
# Usage: DEVICE=mac|ipad|iphone|web [SCREEN=canvas|settings|books|reader|tasks|task|filters|filter|grouped|notes|note|bookmarks|article|bookmark|bookmark-search|article-find] [OUT=path] scripts/screenshot.sh
# Builds the app, launches it with the sample Drawing already open
# (-UITestingOpenSampleDrawing), or with Settings open (SCREEN=settings,
# -UITestingOpenSettings), or with the Books Gallery over sample Books
# (SCREEN=books, -UITestingOpenBooks), or with the first sample Book open in
# the Reader (SCREEN=reader, -UITestingOpenReader), or with the Tasks list over
# sample Tasks (SCREEN=tasks, -UITestingOpenTasks), or with the sheet of the
# first one, which has a Subtask, open over them (SCREEN=task,
# -UITestingOpenTaskSheet), or with the Filters list (SCREEN=filters,
# -UITestingOpenFilters) or one Filter's Tasks (SCREEN=filter,
# -UITestingOpenFilter) over sample Tasks, or with the Notes list over sample
# Notes (SCREEN=notes, -UITestingOpenNotes) or with one open (SCREEN=note,
# -UITestingOpenNote), or with the Bookmarks list over sample Bookmarks
# (SCREEN=bookmarks, -UITestingOpenBookmarks) or the first one's Article in the
# Reader (SCREEN=article, -UITestingOpenArticle) or its Details (SCREEN=bookmark,
# -UITestingOpenBookmarkDetails), and saves a screenshot to $OUT
# (default /tmp/wt-$DEVICE.png). DEVICE=web captures the web app (see
# screenshot-web.sh). On Mac only the app's window is captured.
set -euo pipefail

if [ "${DEVICE:-}" = web ]; then
    exec "$(dirname "$0")/screenshot-web.sh"
fi
cd "$(dirname "$0")/../apple"

DEVICE=${DEVICE:-iphone}
case "${SCREEN:-canvas}" in
canvas) LAUNCH_ARG=-UITestingOpenSampleDrawing WINDOW_NAME= ;;
settings) LAUNCH_ARG=-UITestingOpenSettings WINDOW_NAME=Settings ;;
books) LAUNCH_ARG=-UITestingOpenBooks WINDOW_NAME= ;;
reader) LAUNCH_ARG=-UITestingOpenReader WINDOW_NAME= ;;
tasks) LAUNCH_ARG=-UITestingOpenTasks WINDOW_NAME= ;;
task) LAUNCH_ARG=-UITestingOpenTaskSheet WINDOW_NAME= ;;
filters) LAUNCH_ARG=-UITestingOpenFilters WINDOW_NAME= ;;
filter) LAUNCH_ARG=-UITestingOpenFilter WINDOW_NAME= ;;
grouped) LAUNCH_ARG=-UITestingOpenGroupedFilter WINDOW_NAME= ;;
notes) LAUNCH_ARG=-UITestingOpenNotes WINDOW_NAME= ;;
note) LAUNCH_ARG=-UITestingOpenNote WINDOW_NAME= ;;
bookmarks) LAUNCH_ARG=-UITestingOpenBookmarks WINDOW_NAME= ;;
bookmark) LAUNCH_ARG=-UITestingOpenBookmarkDetails WINDOW_NAME= ;;
article) LAUNCH_ARG=-UITestingOpenArticle WINDOW_NAME= ;;
bookmark-search) LAUNCH_ARG=-UITestingSearchBookmarks WINDOW_NAME= ;;
article-find) LAUNCH_ARG=-UITestingFindInArticle WINDOW_NAME= ;;
*) echo "SCREEN must be canvas, settings, books, reader, tasks, task, filters, filter, grouped, notes, note, bookmarks, article, bookmark, bookmark-search or article-find" >&2; exit 2 ;;
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
    # A locked screen shows no app windows, so there is nothing to capture.
    # Not `grep -q`: quitting early would fail `ioreg` with SIGPIPE, and so the pipe.
    if ioreg -n Root -d1 | grep '"CGSSessionScreenIsLocked"=Yes' >/dev/null; then
        echo "error: the screen is locked; unlock it to take a Mac screenshot" >&2
        exit 1
    fi
    xcodebuild -quiet -scheme pkm -destination 'platform=macOS' -derivedDataPath "$DERIVED" build
    app="$DERIVED/Build/Products/Debug/pkm.app"
    pkill -x 'pkm' 2>/dev/null || true
    while pgrep -xq 'pkm'; do sleep 0.2; done # so its window isn't the one captured
    caffeinate -u -t 1 # wakes the display: a sleeping one shows no windows
    open -n "$app" --args "$LAUNCH_ARG"
    # Capture just the app's window, so other windows never end up in the file.
    # Waits up to 60 s for it, as a busy Mac can take long to launch the app.
    window=
    for _ in $(seq 60); do
        sleep 1
        window=$(osascript -l JavaScript -e '
            ObjC.import("CoreGraphics");
            const list = ObjC.deepUnwrap(ObjC.castRefToObject($.CGWindowListCopyWindowInfo(1, 0))) || [];
            let id = "";
            for (const w of Array.from(list)) {
                if (w.kCGWindowOwnerName === "pkm" && w.kCGWindowLayer === 0 && (w.kCGWindowName || "").includes("'"$WINDOW_NAME"'")) { id = w.kCGWindowNumber; break; }
            }
            id')
        [ -n "$window" ] && break
    done
    if [ -z "$window" ]; then
        pkill -x 'pkm' || true
        echo "error: no pkm window appeared on screen within 60 s" >&2
        exit 1
    fi
    sleep 3 # lets the window finish its first layout and render
    screencapture -x -o -l "$window" "$OUT"
    pkill -x 'pkm' || true
else
    xcrun simctl list devices available | grep -qF "    $SIM (" || xcrun simctl create "$SIM" "$SIM" >/dev/null
    xcodebuild -quiet -scheme pkm -destination "platform=iOS Simulator,name=$SIM" -derivedDataPath "$DERIVED" build
    xcrun simctl boot "$SIM" 2>/dev/null || true
    xcrun simctl bootstatus "$SIM" >/dev/null
    xcrun simctl install "$SIM" "$DERIVED/Build/Products/Debug-iphonesimulator/pkm.app"
    xcrun simctl launch --terminate-running-process "$SIM" com.timhilt.pkm "$LAUNCH_ARG" >/dev/null
    sleep 8
    xcrun simctl io "$SIM" screenshot "$OUT"
fi
echo "$OUT"
