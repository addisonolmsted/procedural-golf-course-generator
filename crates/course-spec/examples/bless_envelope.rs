//! Prints the blake3 fingerprint of the committed provisional envelope.
//! Re-bless after any data edit:
//!   cargo run -p course-spec --example bless_envelope \
//!     > crates/course-spec/data/envelope_provisional.fingerprint
fn main() {
    let bytes = include_bytes!("../data/envelope_provisional.json");
    println!("{}", blake3::hash(bytes).to_hex());
}
