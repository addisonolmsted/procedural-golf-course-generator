//! Rewrite the dictionary fingerprint sidecar with the asset's blake3.
//! Run after every `tools/dictionary/build.py` bake (the builder writes a
//! sha256 placeholder; the loader's interlock is blake3-only).
fn main() {
    let asset = std::path::Path::new("assets/dictionary_v2.bin");
    let bytes = std::fs::read(asset).expect("assets/dictionary_v2.bin (run the builder)");
    let fp = blake3::hash(&bytes).to_hex().to_string();
    std::fs::write(asset.with_extension("fingerprint"), format!("{fp}\n")).unwrap();
    println!("{fp}");
}
