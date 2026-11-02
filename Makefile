# Terminal-only build entry points; no Xcode GUI needed.
SWIFT_SRC := apple/WorkingTitle apple/WorkingTitleTests
XCODEBUILD := cd apple && xcodebuild -quiet -scheme WorkingTitle
XCTEST := scripts/apple-test.sh

# Simulators matching the project's target devices.
SIM_IPHONE := iPhone 15 Pro
SIM_IPAD := iPad Pro (12.9-inch) (4th generation)
SIM ?= $(SIM_IPHONE)

CORE_XCFRAMEWORK := apple/Generated/WorkingTitleCoreFFI.xcframework
CORE_SOURCES := Cargo.toml Cargo.lock scripts/build-core-apple.sh $(shell find crates -type f)

.PHONY: core-apple apple-project build test test-ios lint fmt sim run clean golden-fixtures

# Rust core -> XCFramework + Swift bindings in apple/Generated (rebuilt only
# when the Rust sources change).
core-apple: $(CORE_XCFRAMEWORK)

$(CORE_XCFRAMEWORK): $(CORE_SOURCES)
	scripts/build-core-apple.sh

# The project references the generated files, so they must exist first.
apple-project: core-apple
	cd apple && xcodegen generate --quiet

# Regenerates the project first, so new Swift files are picked up.
build: apple-project
	$(XCODEBUILD) build -destination 'platform=macOS'
	$(XCODEBUILD) build -destination 'generic/platform=iOS Simulator'

test: apple-project
	cargo test --workspace --locked
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

# Regenerates the committed golden fixtures from the pinned Excalidraw checkout
# (see scripts/golden/README.md). Needs Node; `make test` does not.
golden-fixtures:
	cd scripts/golden && npm ci --no-audit --no-fund && node generate.mjs

clean:
	rm -rf apple/WorkingTitle.xcodeproj apple/build apple/Generated target
