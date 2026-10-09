//! Boundary lengths and conversion failures are observed before any C effect.

use super::*;

#[test]
fn byte_length_range_and_empty_capacity_paths_keep_exact_private_cleanup() {
    let artifact = emit_bytes(&capture::reference(), "sum");
    let entry = entry(&artifact, "sum");
    let client = format!(
        r"
int main(void) {{
  size_t lengths[]={{0,1,4095,4096,4097,1,1}};
  for (size_t test=0; test<7; ++test) {{
    struct zryna_c_v0_context context;
    zryna_c_v0_context_initialize(&context);
    struct zryna_c_v0_inputs inputs={{.count=1}};
    struct zryna_c_v0_outcome outcome;
    zryna_rt_o1_handle input={{0}};
    size_t length=lengths[test];
    assert(zryna_rt_o1_vec_allocate(1,length?length:1,&input) == 0);
    int32_t expected=0;
    int32_t *data=(int32_t *)input.pointer;
    for (size_t index=0; index<length; ++index) {{ data[index]=(int32_t)(index%256); expected+=data[index]; }}
    input.length=length;
    if (test == 5) data[0]=-1;
    if (test == 6) data[0]=256;
    memcpy(&inputs.owned[0],&input,sizeof(input));
    fixture_test_reset();
    uint32_t tag={entry}(&context,&inputs,&outcome);
    if (test < 4) {{ assert(tag == 0 && outcome.value == expected && fixture_test_observe().calls == 1); }}
    else {{ assert(tag == 2 && outcome.trap == (test == 4?2U:3U) && fixture_test_observe().calls == 0); }}
    assert(outcome.unresolved == 0 && context.private_unresolved == 0 && allocation_head == NULL);
    assert(outcome.owned.pointer == 0 && context.live == 0 && context.reserved == 0 && context.poisoned == 0);
  }}
  return 0;
}}
"
    );
    run(&artifact, &client, true);
    run_sanitized(&artifact, &client);
}

#[test]
fn signed_count_and_loan_overrun_checks_precede_c_and_release_the_private_prefix() {
    for count in ["-1", "4"] {
        let buffer =
            capture::BUFFER.replace("Ffi.byteLength(loan), out);", &format!("{count}, out);"));
        let artifact =
            emit_bytes(&capture::edited(&buffer, capture::HANDLE, capture::SCALAR), "sum");
        let entry = entry(&artifact, "sum");
        let client = format!(
            r"
int main(void) {{
  struct zryna_c_v0_context context;
  zryna_c_v0_context_initialize(&context);
  struct zryna_c_v0_inputs inputs={{.count=1}};
  struct zryna_c_v0_outcome outcome;
  zryna_rt_o1_handle input={{0}};
  assert(zryna_rt_o1_vec_allocate(1,3,&input) == 0);
  memset((void *)input.pointer,0,12); input.length=3;
  memcpy(&inputs.owned[0],&input,sizeof(input));
  fixture_test_reset();
  assert({entry}(&context,&inputs,&outcome) == 3);
  assert(fixture_test_observe().calls == 0 && allocation_head == NULL);
  assert(outcome.unresolved == 0 && context.poisoned == 1 && outcome.value == 0);
  return 0;
}}
"
        );
        run(&artifact, &client, true);
    }
}

#[test]
fn complete_generated_utf8_validation_accepts_scalar_boundaries_and_rejects_corruption() {
    let artifact = emit_bytes(&capture::reference(), "utf8Sum");
    let entry = entry(&artifact, "utf8Sum");
    let client = format!(
        r"
int main(void) {{
  const uint8_t valid[]={{0,0x7f,0xc2,0x80,0xdf,0xbf,0xe0,0xa0,0x80,0xed,0x9f,0xbf,0xee,0x80,0x80,0xef,0xbf,0xbf,0xf0,0x90,0x80,0x80,0xf4,0x8f,0xbf,0xbf}};
  const uint8_t invalid[][4]={{{{0x80,0,0,0}},{{0xc0,0x80,0,0}},{{0xc2,0,0,0}},{{0xe0,0x9f,0xbf,0}},{{0xed,0xa0,0x80,0}},{{0xf0,0x8f,0xbf,0xbf}},{{0xf4,0x90,0x80,0x80}},{{0xf5,0x80,0x80,0x80}}}};
  const size_t sizes[]={{1,2,1,3,3,4,4,4}};
  for (size_t test=0; test<9; ++test) {{
    struct zryna_c_v0_context context;
    zryna_c_v0_context_initialize(&context);
    struct zryna_c_v0_inputs inputs={{.count=1}};
    struct zryna_c_v0_outcome outcome;
    zryna_rt_o1_handle input={{0}};
    if (test == 0) {{ assert(zryna_rt_o1_string_from_utf8_copy(valid,sizeof(valid),&input) == 0); }}
    else {{
      const uint8_t ascii[]={{'a','a','a','a'}};
      assert(zryna_rt_o1_string_from_utf8_copy(ascii,sizes[test-1],&input) == 0);
      memcpy((void *)input.pointer,invalid[test-1],sizes[test-1]);
    }}
    memcpy(&inputs.owned[0],&input,sizeof(input));
    fixture_test_reset();
    if (test == 0) {{
      int32_t expected=0; for (size_t index=0; index<sizeof(valid); ++index) expected+=valid[index];
      assert({entry}(&context,&inputs,&outcome) == 0 && outcome.value == expected && allocation_head == NULL);
    }} else {{
      assert({entry}(&context,&inputs,&outcome) == 3);
      assert(outcome.trap == 0 && outcome.unresolved == 1 && context.private_unresolved == 1 && context.poisoned == 1);
      assert(fixture_test_observe().calls == 0 && header_for(input.pointer) != NULL);
      assert({entry}(&context,&inputs,&outcome) == 3 && outcome.unresolved == 1);
      /* Explicit test disposal of the unresolved input is not generated cleanup evidence. */
      assert(zryna_rt_o1_string_release(&input) == 0 && allocation_head == NULL);
    }}
  }}
  return 0;
}}
"
    );
    run(&artifact, &client, true);
    run_sanitized(&artifact, &client);
}
