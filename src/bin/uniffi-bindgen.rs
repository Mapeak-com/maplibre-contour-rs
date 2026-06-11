//! Standalone binary that generates the Kotlin/Swift bindings from the
//! compiled library. Build it with the `uniffi-cli` feature, then run e.g.:
//!
//! ```bash
//! cargo run --features uniffi-cli --bin uniffi-bindgen -- \
//!     generate --library target/release/libmaplibre_contour_rs.dylib \
//!     --language kotlin --out-dir bindings/kotlin
//! ```
//!
//! See `CLAUDE.md` → "Mobile bindings" for the full Android/iOS recipe.

fn main() {
    uniffi::uniffi_bindgen_main()
}
