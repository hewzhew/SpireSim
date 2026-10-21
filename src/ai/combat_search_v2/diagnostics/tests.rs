use super::ratio::rounded_ratio;

#[test]
fn rounded_ratio_uses_bounded_precision() {
    assert_eq!(rounded_ratio(0, 0), 0.0);
    assert_eq!(rounded_ratio(10, 3), 3.33);
}
