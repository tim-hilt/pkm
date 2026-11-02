# Terminal-only build entry points; no Xcode GUI needed.
SWIFT_SRC := apple/WorkingTitle apple/WorkingTitleTests
XCODEBUILD := cd apple && xcodebuild -quiet -scheme WorkingTitle
XCTEST := scripts/apple-test.sh

# Simulators matching the project's target devices.
SIM_IPHONE := iPhone 15 Pro
SIM_IPAD := iPad Pro (12.9-inch) (4th generation)
SIM ?= $(SIM_IPHONE)

.PHONY: apple-project build test test-ios lint fmt sim run clean

apple-project:
	cd apple && xcodegen generate --quiet

# Regenerates the project first, so new Swift files are picked up.
build: apple-project
	$(XCODEBUILD) build -destination 'platform=macOS'
	$(XCODEBUILD) build -destination 'generic/platform=iOS Simulator'

test: apple-project
	$(XCTEST) -destination 'platform=macOS'

test-ios: apple-project sim
	$(XCTEST) -destination 'platform=iOS Simulator,name=$(SIM)'

lint:
	swift format lint --strict -r $(SWIFT_SRC)

fmt:
	swift format -i -r $(SWIFT_SRC)

# Creates the SIM device unless it exists.
sim:
	xcrun simctl list devices available | grep -qF '    $(SIM) (' || xcrun simctl create '$(SIM)' '$(SIM)'

# Builds and launches on SIM; screenshot: xcrun simctl io booted screenshot /tmp/wt.png
run: apple-project sim
	$(XCODEBUILD) build -destination 'platform=iOS Simulator,name=$(SIM)' -derivedDataPath build
	xcrun simctl boot '$(SIM)' 2>/dev/null || true
	xcrun simctl install '$(SIM)' apple/build/Build/Products/Debug-iphonesimulator/WorkingTitle.app
	xcrun simctl launch --terminate-running-process '$(SIM)' com.timhilt.workingtitle

clean:
	rm -rf apple/WorkingTitle.xcodeproj apple/build
