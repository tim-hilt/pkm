# syntax=docker/dockerfile:1
# The web app as a static site (issue 16): the core as wasm, the bundle, then
# BusyBox httpd. The build stages run on the build platform; only the last
# stage is of the target platform (linux/arm64 for the Pi).

# 1. The core as wasm in web/generated, built by the same script as `make web-wasm`.
FROM --platform=$BUILDPLATFORM rust:slim AS wasm
ARG BUILDARCH
WORKDIR /src
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*
# The prebuilt wasm-bindgen CLI, at the version of the crate in Cargo.lock (the
# script finds it on PATH and skips `cargo install`).
COPY Cargo.lock ./
RUN version=$(awk '/^name = "wasm-bindgen"$/ {getline; gsub(/[^0-9.]/, ""); print; exit}' Cargo.lock) \
    && arch=$([ "$BUILDARCH" = arm64 ] && echo aarch64 || echo x86_64) \
    && curl -fsSL "https://github.com/wasm-bindgen/wasm-bindgen/releases/download/$version/wasm-bindgen-$version-$arch-unknown-linux-musl.tar.gz" \
    | tar -xz --strip-components=1 -C /usr/local/bin "wasm-bindgen-$version-$arch-unknown-linux-musl/wasm-bindgen"
RUN rustup target add wasm32-unknown-unknown
COPY Cargo.toml ./
COPY crates crates
COPY scripts/build-web-wasm.sh scripts/
# The script copies the sample Drawing and the font from here.
COPY apple/ExcalidrawNative/Resources/sample.excalidraw apple/ExcalidrawNative/Resources/
COPY apple/ExcalidrawNative/Resources/Fonts/Excalifont-Regular.ttf apple/ExcalidrawNative/Resources/Fonts/
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    scripts/build-web-wasm.sh

# 2. The production build in web/dist.
FROM --platform=$BUILDPLATFORM oven/bun:1 AS web
WORKDIR /src/web
# The build only bundles; the dev tools (tsc, vitest, Playwright) stay out.
COPY web/package.json web/bun.lock ./
RUN bun install --frozen-lockfile --production
COPY web/index.html web/tsconfig.json ./
COPY web/scripts scripts
COPY web/src src
COPY web/static static
COPY --from=wasm /src/web/generated generated
RUN bun run build

# 3. The site on port 80.
FROM lipanski/docker-static-website:latest
EXPOSE 80
COPY web/httpd.conf /home/static/httpd.conf
COPY --from=web /src/web/dist /home/static
CMD ["/busybox-httpd", "-f", "-v", "-p", "80"]
