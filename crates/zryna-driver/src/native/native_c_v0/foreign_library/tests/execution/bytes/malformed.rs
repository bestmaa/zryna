//! Controlled actual C metadata defects; no promise about an arbitrary fabricated pointer.

use super::*;

#[test]
fn separate_actual_malformed_byte_results_allocate_no_private_copy_and_release_live_allocations() {
    for (metadata, releases_expected) in [
        ("*out=copy; *count=0;", 1),
        ("*out=copy; *count=2;", 1),
        ("*out=copy; *count=4097;", 1),
        ("free(copy); *out=NULL; *count=length;", 1),
    ] {
        let source = library_source().replace("*out=copy; *count=length;", metadata);
        assert_ne!(source, library_source());
        let client = format!(
            r"
int main(void) {{
  reset_observation();
  struct zryna_c_v0_context context; zryna_c_v0_context_initialize(&context);
  struct zryna_c_v0_inputs inputs={{.count=1}}; struct zryna_c_v0_outcome outcome;
  input_bytes(&inputs,3);
  assert(ENTRY(&context,&inputs,&outcome)==3 && outcome.tag==3);
  assert(outcome.owned.pointer==0 && outcome.value==0 && outcome.trap==0 && outcome.unresolved==0);
  assert(context.poisoned==1 && context.live==0 && context.reserved==0);
  assert(owner_count==3 && attempts==3); /* input + packing + C bytes; no private-copy allocation */
  assert(foreign_count(3,0)=={releases_expected} && foreign_count(3,1)==0);
  assert(live_count()==0 && releases==owner_count);
  return 0;
}}
"
        );
        for sanitized in [false, true] {
            assert!(observe("copied", &source, &client, sanitized).status.success());
        }
    }
}
