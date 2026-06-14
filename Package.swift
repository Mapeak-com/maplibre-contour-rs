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
            url: "https://github.com/Mapeak-com/maplibre-contour-rs/releases/download/v0.2.0/MaplibreContourFFI.xcframework.zip",
            checksum: "2ead791836e57d5665654d65d9d6023799f0960552f194d41584730c442aa317"
        ),
        .target(
            name: "MaplibreContour",
            dependencies: ["MaplibreContourFFI"],
            path: "Sources/MaplibreContour"
        ),
    ]
)
