//! Re-bless helper: prints the seed-1 golden spec artifact to stdout.
//! Run after an INTENTIONAL prior/contract event:
//!   cargo run -p course-spec --example bless_golden \
//!     > crates/course-spec/tests/golden_spec_seed_1.json
//! (also update FINGERPRINT_GOLDEN in src/prior.rs — the fingerprint test
//! fails first and prints the new value).

use course_seed::RunIdentity;
use course_spec::{CourseSpec, SpecOverrides};

fn main() {
    let spec =
        CourseSpec::generate_builtin(RunIdentity::from_seed(1), &SpecOverrides::default()).unwrap();
    print!("{}", spec.canonical_json());
}
