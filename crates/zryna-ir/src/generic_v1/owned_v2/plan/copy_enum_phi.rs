//! Independent Copy enum CFG proofs; no source producer is used.
use super::*;
use authorities::*;
use observation::observe;
use zryna_source::SourceMap;
#[path = "copy_enum_phi/authorities.rs"]
mod authorities;
#[path = "copy_enum_phi/base_fixture_tests.rs"]
mod base_fixture_tests;
#[path = "copy_enum_phi/multiple_root_tests.rs"]
mod multiple_root_tests;
#[path = "copy_enum_phi/observation.rs"]
mod observation;
#[path = "copy_enum_phi/owner_attack_tests.rs"]
mod owner_attack_tests;
#[path = "copy_enum_phi/parallel_fixture_tests.rs"]
mod parallel_fixture_tests;
#[path = "copy_enum_phi/plan_attack_tests.rs"]
mod plan_attack_tests;
#[path = "copy_enum_phi/typed_attack_tests.rs"]
mod typed_attack_tests;
