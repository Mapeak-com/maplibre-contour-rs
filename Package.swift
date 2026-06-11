// swift-tools-version:5.7
import PackageDescription

// The compiled Rust is shipped as an xcframework attached to each GitHub
// Release; the `url`/`checksum` below are rewritten by .github/workflows/
// release.yml for every tag. `Sources/MaplibreContour/maplibre_contour_rs.swift`
// is the generated uniffi wrapper (committed, also refreshed per release).
//
// Consume in Xcode: File → Add Packages → https://github.com/mapeak-com/maplibre-contour-rs
// and pin to a version >= the first release. (main itself points at a
// placeholder release and won't resolve.)
let package = Package(
    name: "MaplibreContour",
    platforms: [.iOS(.v13)],
    products: [
        .library(name: "MaplibreContour", targets: ["MaplibreContour"]),
    ],
    targets: [
        .binaryTarget(
            name: "MaplibreContourFFI",
            url: "https://github.com/Mapeak-com/maplibre-contour-rs/releases/download/v0.1.1/MaplibreContour.xcframework.zip",
            checksum: "9d1cef82b6fb976acf7f75b5debcdd80817a7e765b23191ee7fc7ad4f9f55808"
        ),
        .target(
            name: "MaplibreContour",
            dependencies: ["MaplibreContourFFI"],
            path: "Sources/MaplibreContour"
        ),
    ]
)
