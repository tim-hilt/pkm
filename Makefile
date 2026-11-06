# Terminal-only build entry points; no Xcode GUI needed.
SWIFT_SRC := apple/ExcalidrawNative apple/ExcalidrawNativeTests apple/ExcalidrawNativeUITests
XCODEBUILD := cd apple && xcodebuild -quiet -scheme ExcalidrawNative
XCTEST := scripts/apple-test.sh

# Simulators matching the project's target devices.
SIM_IPHONE := iPhone 15 Pro
SIM_IPAD := iPad Pro (12.9-inch) (4th generation)
SIM ?= $(SIM_IPHONE)

CORE_XCFRAMEWORK := apple/Generated/ExcalidrawNativeCoreFFI.xcframework
CORE_SOURCES := Cargo.toml Cargo.lock scripts/build-core-apple.sh $(shell find crates -type f)

.PHONY: core-apple apple-project build ipa test test-ios test-ui record-references lint fmt check sim run clean golden-fixtures bench visual-parity visual-parity-app screenshot

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

# Builds an unsigned ExcalidrawNative.ipa for sideloading on an iPhone or iPad
# (TARGETED_DEVICE_FAMILY is 1,2). SideStore, AltStore and similar tools
# re-sign it on the device, so no provisioning profile is embedded.
IPA_PRODUCT := apple/build/Build/Products/Release-iphoneos/ExcalidrawNative.app
ipa: apple-project
	$(XCODEBUILD) build -configuration Release -destination 'generic/platform=iOS' \
		-derivedDataPath build CODE_SIGNING_ALLOWED=NO
	rm -rf apple/build/Payload apple/build/ExcalidrawNative.ipa
	mkdir -p apple/build/Payload
	cp -R $(IPA_PRODUCT) apple/build/Payload/
	cd apple/build && zip -qry ExcalidrawNative.ipa Payload
	@echo "apple/build/ExcalidrawNative.ipa"

test: apple-project
	cargo test --workspace --locked
	$(XCTEST) -destination 'platform=macOS'

test-ios: apple-project sim
	$(XCTEST) -destination 'platform=iOS Simulator,name=$(SIM)'

# XCUITests (Gallery → Canvas → back, Gallery → licences); kept out of `make test`, as UI tests on
# macOS need the Accessibility permission.
test-ui: apple-project sim
	SCHEME=ExcalidrawNativeUITests LOG=/tmp/excalidraw-native-ui-test.log $(XCTEST) -destination 'platform=iOS Simulator,name=$(SIM)'

# Re-records the renderer reference images that no longer match (and creates
# missing ones) on macOS; review the diff of apple/ExcalidrawNativeTests/References
# before committing. See docs/renderer-reference-images.md.
record-references: apple-project
	TEST_RUNNER_RECORD_REFERENCES=1 $(XCTEST) -destination 'platform=macOS' ENABLE_APP_SANDBOX=NO

# The pre-commit hook runs this (enable: git config core.hooksPath .githooks).
lint:
	@# One integration-test binary: each extra one costs linking and a ~13 s first-launch scan on macOS.
	@! ls crates/*/tests/*.rs >/dev/null 2>&1 \
		|| { echo 'error: add integration tests as modules of crates/core/tests/integration/main.rs, not as files in tests/'; exit 1; }
	swift format lint --strict -r $(SWIFT_SRC)
	cargo fmt --all --check
	cargo clippy --workspace --all-targets --locked -q -- -D warnings
	@# ADR-0009: the core crate has no dependencies.
	@test "$$(cargo tree -p excalidraw-native-core -e normal --depth 1 --prefix none --locked | wc -l)" -eq 1 \
		|| { echo 'error: crates/core must not have dependencies (ADR-0009)'; exit 1; }

fmt:
	swift format -i -r $(SWIFT_SRC)
	cargo fmt --all

# Fixes formatting, then lints, then runs the Rust tests: cheapest failure first.
# Run this before committing.
check: fmt lint
	cargo test --workspace --locked -q

# Creates the SIM device unless it exists.
sim:
	xcrun simctl list devices available | grep -qF '    $(SIM) (' || xcrun simctl create '$(SIM)' '$(SIM)'

# Builds and launches on SIM; screenshot: xcrun simctl io booted screenshot /tmp/wt.png
run: apple-project sim
	$(XCODEBUILD) build -destination 'platform=iOS Simulator,name=$(SIM)' -derivedDataPath build
	xcrun simctl boot '$(SIM)' 2>/dev/null || true
	xcrun simctl install '$(SIM)' apple/build/Build/Products/Debug-iphonesimulator/ExcalidrawNative.app
	xcrun simctl launch --terminate-running-process '$(SIM)' com.timhilt.excalidrawnative

# Regenerates the committed golden fixtures from the pinned Excalidraw checkout
# (see scripts/golden/README.md). Needs Node; `make test` does not.
golden-fixtures:
	cd scripts/golden && npm ci --no-audit --no-fund && node generate.mjs

# Visual parity review (docs/visual-parity.md): regenerates the fixtures and
# exports them from excalidraw.com (needs network and Google Chrome) and from
# the app, into docs/visual-parity.
visual-parity: visual-parity-app
	cd scripts/visual-parity && npm ci --no-audit --no-fund && node fixtures.mjs && node excalidraw.mjs

visual-parity-app: apple-project
	TEST_RUNNER_RECORD_VISUAL_PARITY=1 $(XCTEST) -destination 'platform=macOS' ENABLE_APP_SANDBOX=NO \
		-only-testing:ExcalidrawNativeTests/VisualParityTests

# Measures the core's open time for the 1,000-Element Benchmark Scene and its
# share of one input event in the 5,000-Element one.
# See docs/performance-benchmarks.md for the full procedure.
bench:
	cargo test --release --locked -q -p excalidraw-native-core --test integration -- --ignored --nocapture --test-threads=1 benchmarks::

clean:
	rm -rf apple/ExcalidrawNative.xcodeproj apple/build apple/Generated target

# Screenshot of the Canvas with the sample Drawing open, for looking at layout:
# make screenshot DEVICE=mac|ipad|iphone [OUT=/tmp/wt-<device>.png]
DEVICE ?= iphone
screenshot: apple-project
	DEVICE=$(DEVICE) OUT='$(OUT)' scripts/screenshot.sh
