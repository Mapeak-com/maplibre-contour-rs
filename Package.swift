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
    platforms: [.iOS(.v13)],
    products: [
        .library(name: "MaplibreContour", targets: ["MaplibreContour"]),
    ],
    targets: [
        .binaryTarget(
            name: "MaplibreContourFFI",
            url: "https://github.com/Mapeak-com/maplibre-contour-rs/releases/download/v0.4.2/MaplibreContourFFI.xcframework.zip",
            checksum: "99f80b483e11cacd70c9f04b0272fe825d69ecf3805832e995216177796f6b38"
        ),
        .target(
            name: "MaplibreContour",
            dependencies: ["MaplibreContourFFI"],
            path: "Sources/MaplibreContour"
        ),
    ]
)
