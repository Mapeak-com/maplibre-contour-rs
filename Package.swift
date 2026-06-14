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
            url: "https://github.com/Mapeak-com/maplibre-contour-rs/releases/download/v0.3.0/MaplibreContourFFI.xcframework.zip",
            checksum: "d07573b3cac3270d21d5b853235e6c3c6c01cfbf72e80c1d174de40a1a2ef2f6"
        ),
        .target(
            name: "MaplibreContour",
            dependencies: ["MaplibreContourFFI"],
            path: "Sources/MaplibreContour"
        ),
    ]
)
