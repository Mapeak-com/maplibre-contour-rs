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
| Contour tracing | [`contour`](src/contour.rs) | `contour` |
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

The crate has no native C dependencies, so it cross-compiles cleanly. With the
`ffi` feature it exposes a [uniffi](https://mozilla.github.io/uniffi-rs/)
interface for Kotlin and Swift (see the `ffi` module docs for usage).

**Prebuilt artifacts** — Android (`jniLibs` + Kotlin) and iOS (`.xcframework` +
Swift) bundles are attached to every [GitHub Release](../../releases), built by
[`.github/workflows/release.yml`](.github/workflows/release.yml). Tag a commit
(`git tag v0.1.0 && git push --tags`) to produce them; consuming apps just
download the bundle for their platform.

To generate bindings locally instead:

```bash
rustup target add aarch64-apple-ios aarch64-linux-android  # etc.
cargo build --release --features ffi
cargo run --features uniffi-cli --bin uniffi-bindgen -- generate \
    --library target/release/libmaplibre_contour_rs.dylib \
    --language kotlin --out-dir bindings
```

## License

Dual-licensed under MIT or Apache-2.0.
