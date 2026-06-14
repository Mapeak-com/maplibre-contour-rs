# maplibre-contour-rs

Generate contour-line **vector tiles (MVT)** on the fly from raster-DEM tiles
(Mapbox Terrain-RGB or Terrarium), on Android and iOS. A Rust port of
[`maplibre-contour`](https://github.com/onthegomap/maplibre-contour) with
[uniffi](https://mozilla.github.io/uniffi-rs/) Kotlin/Swift bindings — no
native C dependencies, nothing to publish to Maven / CocoaPods / crates.io.

You **implement a fetcher** that returns DEM bytes for a tile URL (so your own
HTTP stack, cache, or interceptor applies), set the contour options, and call
`tile(z, x, y)` to get back MVT bytes — ready to serve to MapLibre as a vector
source. Contours above the DEM's max zoom are overzoomed from the ancestor tile
automatically, and lines stay continuous across tile seams.

## iOS — Swift Package Manager

Add the package in Xcode (**File → Add Packages →**
`https://github.com/mapeak-com/maplibre-contour-rs`, pinned to a version tag),
then:

```swift
import MaplibreContour

// Return DEM PNG/WebP bytes for a resolved tile URL (use your intercepted
// URLSession / on-disk PMTiles / etc.); return nil where there's no data.
final class HttpDemFetcher: DemTileFetcher {
    func fetch(url: String) throws -> Data? {
        guard let u = URL(string: url) else { return nil }
        return try? Data(contentsOf: u)
    }
}

var config = defaultConfig()
config.demUrlPattern = "https://example.com/dem/{z}/{x}/{y}.png"
config.demMaxZoom = 11
config.overzoom = 1
config.thresholds = parseThresholdSpec("11*200*1000~12*10*100~13*10*100")

let tiler = ContourTiler(fetcher: HttpDemFetcher(), config: config)
let mvt: Data = try tiler.tile(z: 14, x: 9000, y: 6000)
```

## Android — JitPack

```groovy
repositories { maven { url 'https://jitpack.io' } }
dependencies {
    implementation 'com.github.mapeak-com.maplibre-contour-rs:contour:v0.1.0'
}
```

```kotlin
// Fetch through OkHttp (so an interceptor — e.g. serving from PMTiles — applies);
// return null where there's no data.
class HttpDemFetcher(private val client: OkHttpClient) : DemTileFetcher {
    override fun fetch(url: String): ByteArray? {
        val resp = client.newCall(Request.Builder().url(url).build()).execute()
        return resp.use { if (it.isSuccessful) it.body?.bytes() else null }
    }
}

val config = defaultConfig().copy(
    demUrlPattern = "https://example.com/dem/{z}/{x}/{y}.png",
    encoding = Encoding.TERRARIUM,
    demMaxZoom = 11u,
    overzoom = 1u,
    thresholds = parseThresholdSpec("11*200*1000~12*10*100~13*10*100"),
)

val tiler = ContourTiler(HttpDemFetcher(client), config)
val mvt: ByteArray = tiler.tile(14u, 9000u, 6000u)
```

Serve the returned MVT bytes to MapLibre through your map's tile provider (a
custom protocol / request interceptor that calls `tiler.tile(z, x, y)`), then
add a `vector` source + line layers styled on the `ele`/`level` attributes.

`ContourTiler` is thread-safe — build one and call `tile` off the main thread.

## Configuration

`defaultConfig()` returns Terrarium / 256 px / 4096 extent; override what you
need:

| Field | Meaning |
|------|---------|
| `demUrlPattern` | DEM tile URL with `{z}`/`{x}`/`{y}` (and `{-y}` for TMS); resolved and passed to your fetcher. |
| `encoding` | `Terrarium` or `Mapbox` (Terrain-RGB). |
| `thresholds` | Per-zoom contour spacing. `parseThresholdSpec("11*200*1000~…")` = `zoom*minor*major`; minor lines are traced, multiples of `major` are tagged `level = 1`. |
| `multiplier` | Elevation unit scale before contouring (`1.0` = metres, `3.28084` = feet). |
| `demMaxZoom` / `overzoom` | DEM availability + overzoom; the DEM is sampled at `min(z - overzoom, demMaxZoom)`. |
| `layerName` / `elevationKey` / `levelKey` | MVT layer name and the `ele` / `level` attribute keys. |

## Use from Rust

For server-side or other non-mobile use, depend on it straight from GitHub
(no registry needed) and implement [`TileSource`](src/source.rs):

```toml
[dependencies]
maplibre-contour-rs = { git = "https://github.com/mapeak-com/maplibre-contour-rs", tag = "v0.1.0" }
```

```rust,no_run
use maplibre_contour_rs::{ContourConfig, ContourTiler, TileCoord};
use maplibre_contour_rs::source::MockTileSource;

let source = MockTileSource::default(); // your TileSource here
let tiler = ContourTiler::new(source, ContourConfig::default());
let mvt: Vec<u8> = tiler.tile(TileCoord::new(12, 2048, 1361))?;
# Ok::<(), maplibre_contour_rs::Error>(())
```

API docs are generated from the source — run `cargo doc --open`.

## Architecture

```
TileCoord ──▶ fetch 3x3 neighborhood ──▶ decode DEM ──▶ assemble buffered grid
          ──▶ one-pass marching-squares contours ──▶ encode MVT ──▶ bytes
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
| Mobile bindings | [`ffi`](src/ffi.rs) | `uniffi` |

The contour engine is a one-pass port of maplibre-contour's `isolines.ts` (a
single grid scan for all thresholds), validated against its golden tests in
[`tests/isolines.rs`](tests/isolines.rs). See [`CLAUDE.md`](./CLAUDE.md) for
design notes and follow-up ideas.

## Building & releasing

```bash
cargo build && cargo test                 # Rust core
./scripts/build-xcframework.sh            # iOS/macOS xcframework + Swift wrapper
( cd android && ./gradlew :contour:assembleRelease )   # Android AAR
```

CI ([`.github/workflows/ci.yml`](.github/workflows/ci.yml)) builds the Rust
core, the Android AAR, and the iOS XCFramework + SwiftPM package on every push.
The **Release (bump version)** workflow
([`release.yml`](.github/workflows/release.yml)) bumps the version, builds the
artifacts, pins `Package.swift` to the release asset on the tag, and attaches
the AAR + XCFramework to the [GitHub Release](../../releases); JitPack then
serves the prebuilt AAR. `main` keeps the path-based `Package.swift` for local
dev.

## License

Dual-licensed under MIT or Apache-2.0.
