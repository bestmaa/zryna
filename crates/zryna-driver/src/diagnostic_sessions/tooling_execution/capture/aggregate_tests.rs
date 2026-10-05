use super::validate_sizes;

#[test]
fn nine_file_aggregate_exact_limit_and_first_extra() {
    let maximum = 16 * 1_024 * 1_024;
    let sizes = [maximum - 8, 1, 1, 1, 1, 1, 1, 1, 1];
    validate_sizes(sizes).expect("exact aggregate limit");
    let mut extra = sizes;
    extra[8] += 1;
    let error = validate_sizes(extra).expect_err("first aggregate byte beyond the limit");
    assert_eq!(error.code(), "ZRYNA-D3001");
    assert!(error.message().contains("exceeds its byte limit"));
}

#[test]
fn nine_file_aggregate_arithmetic_overflow_rejects() {
    let error = validate_sizes([usize::MAX, 1, 0, 0, 0, 0, 0, 0, 0])
        .expect_err("checked aggregate arithmetic");
    assert_eq!(error.code(), "ZRYNA-D3001");
    assert!(error.message().contains("overflowed"));
}

#[test]
fn aggregate_is_not_individual_material_authentication() {
    validate_sizes([0; 9]).expect("only arithmetic is exercised here");
    assert!(validate_sizes([16 * 1_024 * 1_024; 9]).is_err());
}
