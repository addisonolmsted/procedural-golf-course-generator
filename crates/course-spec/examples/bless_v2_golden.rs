//! Prints the seed-1 v2 spec golden:
//!   cargo run -p course-spec --example bless_v2_golden \
//!     > crates/course-spec/tests/golden_spec_v2_seed_1.json
fn main() {
    let id = course_seed::RunIdentity::from_seed(1);
    let s = course_spec::v2::SiteSpec::generate_builtin(id, &Default::default());
    print!("{}", s.canonical_json());
}
