#!/bin/sh
# Prints the app version: the latest git tag without its "v", or 0.1.0 before
# the first release. Releases are tagged with the next version from `make release-notes`.
git describe --tags --abbrev=0 --match 'v[0-9]*' 2>/dev/null | sed 's/^v//' | grep . || echo 0.1.0
