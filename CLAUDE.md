# CLAUDE.md

Guidance for working on `maplibre-contour-rs` with Claude Code.

## What this project is

A Rust reimplementation of [`maplibre-contour`](https://github.com/onthegomap/maplibre-contour).
Given an XYZ tile coordinate, it fetches the corresponding raster-DEM tile plus
its 8 neighbors, decodes elevation, traces contour lines, and encodes them as a
Mapbox Vector Tile. Target use: embed in a mobile app (Android/iOS) and serve
contour tiles locally.

Keep parity with maplibre-contour's behavior where it makes sense — when unsure
how something should behave, check that repo first.

## Architecture (where things live)

Each pipeline stage is a single file (consolidated from the original
`tile/`, `dem/`, `contour/`, `mvt/` directories). All stages below are
**implemented and tested**.

- `config.rs` — `Encoding`, `ContourConfig`, and per-zoom `ThresholdRule`
  (`intervals[0]` = minor; coarser entries tag higher `level`). Also
  `parse_thresholds` (`"11*200*1000~…"`), `thresholds_for(z)`, `source_zoom(z)`.
- `tile.rs` — `TileCoord` (XYZ, x-wrap / y-clamp) and `Neighborhood` (3x3).
- `dem.rs` — `DemGrid` (elevation buffer) and `decode_tile` (PNG → RGBA8 → grid).
- `buffer.rs` — `sample_buffered`: bilinear sample of the DEM at `source_zoom`
  over the tile + margin. One path covers both seam-buffering and **overzoom**
  (sampling a coarser ancestor when `z > dem_max_zoom`), like maplibre-contour's
  `fetchDem` + `combineNeighbors` + subsample.
- `cache.rs` — `DemCache`, LRU of decoded tiles.
- `source.rs` — `TileSource` trait, `MockTileSource`, and `UrlTemplate`
  (`{z}/{x}/{y}`, `{-y}`).
- `contour.rs` — one-pass marching squares ported from maplibre-contour's
  `isolines.ts` (no external deps): `generate_isolines` (the engine, covered by
  `tests/isolines.rs`) + `contour_tile` (samples the buffered grid, scales by
  `multiplier`, tags major/minor `level`).
- `mvt.rs` — `encode_mvt`: grid px → extent space, one layer with `ele`/`level`,
  geometry via geozero's `ToMvt`, serialized with prost.
- `pipeline.rs` — `ContourTiler` resolves `source_zoom` + the active threshold
  rule, then samples → contours → encodes.
- `ffi.rs` — uniffi bindings (`--features ffi`): host-implemented
  `DemTileFetcher` (gets the resolved URL), `ContourTiler`, `default_config`,
  `parse_threshold_spec`. Usage docs live in the module's rustdoc.

Tests: per-module unit tests plus `tests/pipeline.rs` (seam continuity +
overzoom-from-ancestor).

## Follow-up ideas (not yet done)

1. **Built-in HTTP `TileSource`** (behind a feature) for non-FFI/server use.
2. **Benchmarks** (criterion) on the decode→contour→encode hot path; it runs
   on-device, so watch allocations.

## Mobile bindings

Implemented in `ffi.rs` behind the `ffi` feature (uniffi); usage examples live
in that module's rustdoc. Key points:

- The surface is intentionally tiny: `config.dem_url_pattern` holds the DEM URL
  template, the host-implemented `DemTileFetcher` returns bytes for a resolved
  URL (so an HTTP interceptor still fires), and `ContourTiler::tile(z, x, y)`
  returns the MVT `Vec<u8>`. `ContourConfig`/`ThresholdRule`/`Encoding` cross as
  records/enums; `parse_threshold_spec` mirrors the `dem-contour://` query.
- The `uniffi-bindgen` binary (built with `--features uniffi-cli`) generates the
  Kotlin/Swift sources from the compiled library; the CLI deps are gated so a
  plain `--features ffi` mobile build stays lean.
- Packaging mirrors the proven `Mapeak-com/pmtiles-mobile` setup (`ci.yml`
  builds the Rust core, the Android AAR, and the iOS xcframework on every push):
  - **iOS / SwiftPM** — `scripts/build-xcframework.sh` builds the `.a` per Apple
    target (iOS + macOS, so CI `swift build` links), generates the committed
    `Sources/MaplibreContour/maplibre_contour_rs.swift`, and assembles
    `artifacts/MaplibreContourFFI.xcframework`. `Package.swift` is path-based on
    `main`; the release job pins it to the release `url`/`checksum` on the tag.
  - **Android / JitPack** — the self-contained `android/` Gradle project (module
    `:contour`, committed wrapper) cross-compiles the `.so` per ABI via
    `cargo-ndk` and generates the UniFFI Kotlin in the Gradle build. `jitpack.yml`
    runs the same `:contour:publishToMavenLocal`. `.cargo/config.toml` forces
    16 KB ELF alignment (Android 15+); the JNA `@aar` + disabled module metadata
    avoid `UnsatisfiedLinkError`. `uniffi.toml` sets the Kotlin package.
  - **Release** — `release.yml` is a manual `workflow_dispatch` (patch/minor/
    major); it computes the next version from git tags, `cargo set-version`s
    `Cargo.toml`, builds, pins `Package.swift`, and pushes the tag only.
- Keep the core crate free of native C deps (no GDAL/GEOS `geozero` features),
  otherwise cross-compilation gets painful.

## Conventions

- `cargo fmt` + `cargo clippy -D warnings` must pass (CI enforces this).
- Prefer `f32` for elevation grids (memory matters on-device); use `f64` only in
  geometry/coordinate transforms.
- Keep stages independently testable; the pipeline should stay thin.
- Don't add heavy dependencies without a reason — this ships on phones.

## Reference

- maplibre-contour: https://github.com/onthegomap/maplibre-contour
- MVT spec: https://github.com/mapbox/vector-tile-spec
- Terrarium encoding: `elevation = R*256 + G + B/256 - 32768`
- Mapbox Terrain-RGB: `elevation = -10000 + (R*65536 + G*256 + B) * 0.1`
