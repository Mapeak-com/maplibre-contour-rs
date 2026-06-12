// swift-tools-version:5.9
import PackageDescription

// `main` references the locally-built xcframework by path (run
// `scripts/build-xcframework.sh` first); the Release workflow rewrites the
// binaryTarget to `.binaryTarget(url:checksum:)` on the tagged commit, so a
// version tag resolves without building anything.
//
// Consume in Xcode: File → Add Packages → https://github.com/mapeak-com/maplibre-contour-rs
// pinned to a version tag.
let package = Package(
    name: "MaplibreContour",
    platforms: [.iOS(.v13), .macOS(.v11)],
    products: [
        .library(name: "MaplibreContour", targets: ["MaplibreContour"]),
    ],
    targets: [
        .binaryTarget(
            name: "MaplibreContourFFI",
            url: "https://github.com/Mapeak-com/maplibre-contour-rs/releases/download/v0.1.2/MaplibreContourFFI.xcframework.zip",
            checksum: "334d1f8df7af2cb7f9d800f4a19219b0b6ff4bb054b517e235cdfc937cd5d92e"
        ),
        .target(
            name: "MaplibreContour",
            dependencies: ["MaplibreContourFFI"],
            path: "Sources/MaplibreContour"
        ),
    ]
)
