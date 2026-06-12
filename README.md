# maplibre-contour-rs

A Rust port of [`maplibre-contour`](https://github.com/onthegomap/maplibre-contour):
generate contour-line **vector tiles (MVT)** on the fly from raster-DEM tiles
(Mapbox Terrain-RGB or Terrarium encoding). The core is pure Rust so it can be
embedded in an Android/iOS app via FFI.

> **Status: working.** The full pipeline is implemented and tested end-to-end,
> including the seam-continuity guarantee and Kotlin/Swift bindings. See
> [`CLAUDE.md`](./CLAUDE.md) for design notes and follow-up ideas (HTTP source,
> overzoom, benchmarks).

## Pipeline

```
TileCoord ──▶ fetch 3x3 neighborhood ──▶ decode DEM ──▶ assemble buffered grid
          ──▶ marching-squares contours ──▶ transform + encode MVT ──▶ bytes
```

| Stage | Module | Crate |
|------|--------|-------|
| Tile / neighbor math, URL templates | [`tile`](src/tile.rs), [`source`](src/source.rs) | — |
| Decode DEM PNG + elevation grid | [`dem`](src/dem.rs) | `image` |
| Buffered + overzoomed sampling | [`buffer`](src/buffer.rs) | — |
| Decoded-tile cache | [`cache`](src/cache.rs) | `lru` |
| Per-zoom thresholds / config | [`config`](src/config.rs) | — |
| Contour tracing (one-pass) | [`contour`](src/contour.rs) | — |
| MVT encoding | [`mvt`](src/mvt.rs) | `geozero` |
| Orchestration | [`pipeline`](src/pipeline.rs) | — |
| Mobile bindings | [`ffi`](src/ffi.rs) (`--features ffi`) | `uniffi` |

Parity with maplibre-contour: a `{z}/{x}/{y}` DEM URL pattern, per-zoom
`thresholds` (`[minor, major]` spacings, parseable from `"11*200*1000~…"`), an
elevation `multiplier` (e.g. metres → feet), and **overzoom** — contours above
`dem_max_zoom` are sampled from the ancestor DEM (`source_zoom = min(z - overzoom, dem_max_zoom)`).

## Quick start

```bash
cargo build
cargo test                       # unit + seam-continuity integration tests
cargo run --example generate_tile
```

The example builds a synthetic "hill" DEM and runs the whole pipeline, printing
the size of the encoded MVT tile.

## Usage

Implement [`TileSource`](src/source.rs) to supply DEM PNG bytes, then ask the
tiler for a coordinate:

```rust,no_run
use maplibre_contour_rs::{ContourConfig, ContourTiler, TileCoord};
use maplibre_contour_rs::source::MockTileSource;

let source = MockTileSource::default(); // your source here
let tiler = ContourTiler::new(source, ContourConfig::default());
let mvt: Vec<u8> = tiler.tile(TileCoord::new(12, 2048, 1361))?;
# Ok::<(), maplibre_contour_rs::Error>(())
```

The tiler fetches the tile plus its neighbors, stitches a buffered (and, above
`dem_max_zoom`, overzoomed) elevation grid, traces marching-squares isolines at
each threshold, and encodes one MVT layer with `ele` and `level` attributes.

API docs are generated from the source — run `cargo doc --open`.

## Using it as a dependency

No registry publishing required — depend on it straight from GitHub and pin to a
release tag:

```toml
[dependencies]
maplibre-contour-rs = { git = "https://github.com/mapeak-com/maplibre-contour-rs", tag = "v0.1.0" }
```

## Mobile (Android & iOS)

The crate exposes a [uniffi](https://mozilla.github.io/uniffi-rs/) interface
behind the `ffi` feature (see the `ffi` module docs for the Kotlin/Swift API).
Each release publishes consumable packages — no Maven/CocoaPods/crates.io
needed.

### iOS — Swift Package Manager

In Xcode: **File → Add Packages →** `https://github.com/mapeak-com/maplibre-contour-rs`,
pinned to a version tag. [`Package.swift`](Package.swift) vends the compiled
`.xcframework` (downloaded from the release on a tag) plus the generated Swift
wrapper.

```swift
import MaplibreContour
let tiler = ContourTiler(fetcher: myFetcher, config: defaultConfig())
```

### Android — JitPack

Add JitPack and the dependency:

```groovy
repositories { maven { url 'https://jitpack.io' } }
dependencies {
    implementation 'com.github.mapeak-com.maplibre-contour-rs:contour:v0.1.0'
}
```

JitPack builds [`android/`](android/contour/build.gradle.kts) from the tag,
cross-compiling the Rust `.so` per ABI with `cargo-ndk` and generating the
UniFFI Kotlin bindings during the Gradle build (JNA comes transitively).

### How releases are produced

Run the **Release (bump version)** workflow
([`.github/workflows/release.yml`](.github/workflows/release.yml)) and pick
`patch`/`minor`/`major`. It bumps the version, builds the iOS XCFramework, pins
`Package.swift` to the release asset (url + checksum) on the tagged commit,
pushes the tag, and attaches the XCFramework to the
[GitHub Release](../../releases). JitPack builds the Android AAR from the same
tag on first request. `main` keeps the path-based `Package.swift` for local dev.

CI ([`.github/workflows/ci.yml`](.github/workflows/ci.yml)) mirrors this: every
push builds the Rust core, the Android AAR (`./gradlew publishToMavenLocal`, the
same command JitPack runs), and the iOS XCFramework + SwiftPM package — so
packaging/dependency breakage is caught before a release.

To build the bindings/xcframework locally:

```bash
./scripts/build-xcframework.sh            # iOS/macOS xcframework + Swift wrapper
( cd android && ./gradlew :contour:assembleRelease )   # Android AAR
```

## License

Dual-licensed under MIT or Apache-2.0.
