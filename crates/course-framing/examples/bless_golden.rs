//! Re-bless helper: prints the seed-1 golden framing artifact to stdout.
//! Run after an INTENTIONAL contract event (FRAMING_VERSION bump, draw
//! transcript change, framing.* prior change):
//!   cargo run -p course-framing --example bless_golden \
//!     > crates/course-framing/tests/golden_framing_seed_1.json

use course_framing::generate;
use course_seed::RunIdentity;
use course_spec::{CourseSpec, SpecOverrides};

fn main() {
    let spec =
        CourseSpec::generate_builtin(RunIdentity::from_seed(1), &SpecOverrides::default()).unwrap();
    print!("{}", generate(&spec).canonical_json());
}
