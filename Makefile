# Terminal-only build entry points; no Xcode GUI needed.
SWIFT_SRC := apple/pkm apple/pkmTests apple/pkmUITests
XCODEBUILD := cd apple && xcodebuild -quiet -scheme pkm
XCTEST := scripts/apple-test.sh

# Simulators matching the project's target devices.
SIM_IPHONE := iPhone 15 Pro
SIM_IPAD := iPad Pro (12.9-inch) (4th generation)
SIM ?= $(SIM_IPHONE)

CORE_XCFRAMEWORK := apple/Generated/pkmCoreFFI.xcframework
CORE_SOURCES := Cargo.toml Cargo.lock scripts/build-core-apple.sh $(shell find crates -type f)

.PHONY: web-wasm web-install web-lint web-test web-test-browser web-record-references web-fmt web-dev core-apple apple-project build ipa test test-ios test-ui record-references lint fmt check release-notes sim run clean golden-fixtures bench visual-parity visual-parity-app screenshot

# The web app (web/): the core as wasm in web/generated (rebuilt only when the
# Rust sources change), Bun for packages and the dev server.
WEB_WASM := web/generated/pkm_web.js

web-wasm: $(WEB_WASM)

$(WEB_WASM): $(CORE_SOURCES) scripts/build-web-wasm.sh
	scripts/build-web-wasm.sh

web-install: web/node_modules

web/node_modules: web/package.json web/bun.lock
	cd web && bun install --frozen-lockfile
	@touch web/node_modules

web-fmt: web-install
	cd web && bun run format

# Cheapest first; types need the generated wasm bindings.
web-lint: web-install
	cd web && bun run format:check && bun run lint
	$(MAKE) web-wasm
	cd web && bun run typecheck

# Vitest on Node, against the real wasm.
web-test: web-install web-wasm
	cd web && bun run test

# Renderer tests in Chromium with WebGPU (reference images, pixel probes); kept
# out of `make check` as they need a GPU. Run `cd web && bunx playwright-core
# install chromium` once. See docs/web-renderer-reference-images.md.
web-test-browser: web-install web-wasm
	cd web && bun run test:browser

# Re-records the web renderer reference images that no longer match (and
# creates missing ones); review the diff of web/references before committing.
web-record-references: web-install web-wasm
	cd web && RECORD_REFERENCES=1 bun run test:browser

# Dev server: http://localhost:3000
web-dev: web-install web-wasm
	cd web && bun run dev

# Rust core -> XCFramework + Swift bindings in apple/Generated (rebuilt only
# when the Rust sources change).
core-apple: $(CORE_XCFRAMEWORK)

$(CORE_XCFRAMEWORK): $(CORE_SOURCES)
	scripts/build-core-apple.sh

# The project references the generated files, so they must exist first.
apple-project: core-apple
	cd apple && MARKETING_VERSION=$$(../scripts/version.sh) xcodegen generate --quiet

# Regenerates the project first, so new Swift files are picked up.
build: apple-project
	$(XCODEBUILD) build -destination 'platform=macOS'
	$(XCODEBUILD) build -destination 'generic/platform=iOS Simulator'

# Builds an unsigned pkm.ipa for sideloading on an iPhone or iPad
# (TARGETED_DEVICE_FAMILY is 1,2). SideStore, AltStore and similar tools
# re-sign it on the device, so no provisioning profile is embedded.
IPA_PRODUCT := 'apple/build/Build/Products/Release-iphoneos/pkm.app'
ipa: apple-project
	$(XCODEBUILD) build -configuration Release -destination 'generic/platform=iOS' \
		-derivedDataPath build CODE_SIGNING_ALLOWED=NO
	rm -rf apple/build/Payload apple/build/pkm.ipa
	mkdir -p apple/build/Payload
	cp -R $(IPA_PRODUCT) apple/build/Payload/
	cd apple/build && zip -qry pkm.ipa Payload
	@echo "apple/build/pkm.ipa"

test: apple-project
	cargo test --workspace --locked
	$(XCTEST) -destination 'platform=macOS'

test-ios: apple-project sim
	$(XCTEST) -destination 'platform=iOS Simulator,name=$(SIM)'

# XCUITests (Gallery → Canvas → back); kept out of `make test`, as UI tests on
# macOS need the Accessibility permission.
test-ui: apple-project sim
	SCHEME=pkmUITests LOG=/tmp/pkm-ui-test.log $(XCTEST) -destination 'platform=iOS Simulator,name=$(SIM)'

# Re-records the renderer reference images that no longer match (and creates
# missing ones) on macOS; review the diff of apple/pkmTests/References
# before committing. See docs/renderer-reference-images.md.
record-references: apple-project
	TEST_RUNNER_RECORD_REFERENCES=1 $(XCTEST) -destination 'platform=macOS' ENABLE_APP_SANDBOX=NO

# The pre-commit hook runs this (enable: git config core.hooksPath .githooks).
lint:
	swift format lint --strict -r $(SWIFT_SRC)
	cargo fmt --all --check
	cargo clippy --workspace --all-targets --locked -q -- -D warnings
	@# ADR-0009, ADR-0017: pkm-base has no dependencies, pkm-drawing and pkm-tasks only pkm-base, pkm-notes also unicode-normalization (Search folds accents; Books has it already).
	@test "$$(cargo tree -p pkm-base -e normal --depth 1 --prefix none --locked | wc -l)" -eq 1 \
		|| { echo 'error: crates/base must not have dependencies (ADR-0009)'; exit 1; }
	@test "$$(cargo tree -p pkm-drawing -e normal --depth 1 --prefix none --locked | cut -d' ' -f1 | sort | tr '\n' ' ')" = "pkm-base pkm-drawing " \
		|| { echo 'error: crates/drawing may only depend on pkm-base (ADR-0009)'; exit 1; }
	@test "$$(cargo tree -p pkm-tasks -e normal --depth 1 --prefix none --locked | cut -d' ' -f1 | sort | tr '\n' ' ')" = "pkm-base pkm-tasks " \
		|| { echo 'error: crates/tasks may only depend on pkm-base (ADR-0017)'; exit 1; }
	@test "$$(cargo tree -p pkm-notes -e normal --depth 1 --prefix none --locked | cut -d' ' -f1 | sort | tr '\n' ' ')" = "pkm-base pkm-notes unicode-normalization " \
		|| { echo 'error: crates/notes may only depend on pkm-base and unicode-normalization (ADR-0017)'; exit 1; }

fmt: web-fmt
	swift format -i -r $(SWIFT_SRC)
	cargo fmt --all

# The web app's formatting, lint, wasm build and Node tests, then fixes the
# native formatting, lints and runs the Rust tests: cheapest failure first.
# Run this before committing.
check: web-lint web-test fmt lint
	cargo test --workspace --locked -q

# Prints the next version and the changelog since the last tag (cliff.toml);
# changes nothing. Tag a release with that version to ship it.
release-notes:
	@git cliff --bump --unreleased --strip header 2>/dev/null | sed '/^$$/N;/^\n$$/D'
	@echo "next version: $$(git cliff --bumped-version 2>/dev/null)"

# Creates the SIM device unless it exists.
sim:
	xcrun simctl list devices available | grep -qF '    $(SIM) (' || xcrun simctl create '$(SIM)' '$(SIM)'

# Builds and launches on SIM; screenshot: xcrun simctl io booted screenshot /tmp/wt.png
run: apple-project sim
	$(XCODEBUILD) build -destination 'platform=iOS Simulator,name=$(SIM)' -derivedDataPath build
	xcrun simctl boot '$(SIM)' 2>/dev/null || true
	xcrun simctl install '$(SIM)' 'apple/build/Build/Products/Debug-iphonesimulator/pkm.app'
	xcrun simctl launch --terminate-running-process '$(SIM)' com.timhilt.pkm

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
		-only-testing:pkmTests/VisualParityTests

# Measures the core's open time for the 1,000-Element Benchmark Scene and its
# share of one input event in the 5,000-Element one.
# See docs/performance-benchmarks.md for the full procedure.
bench:
	cargo test --release --locked -q -p pkm-drawing --lib -- --ignored --nocapture --test-threads=1 benchmarks::

clean:
	rm -rf apple/pkm.xcodeproj apple/build apple/Generated target web/generated web/dist

# Screenshot of the Canvas with the sample Drawing open, for looking at layout:
# make screenshot DEVICE=mac|ipad|iphone|web [SCREEN=canvas|settings|books|reader|tasks|task|filters|filter|grouped|notes|note|bookmarks|article] [OUT=/tmp/wt-<device>.png]
DEVICE ?= iphone
screenshot: $(if $(filter web,$(DEVICE)),web-install web-wasm,apple-project)
	DEVICE=$(DEVICE) SCREEN='$(SCREEN)' OUT='$(OUT)' scripts/screenshot.sh
