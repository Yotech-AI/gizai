use gizai_core::sortkey::key_after;

#[test]
fn keys_after_always_increase() {
    assert_eq!(key_after(None), "a0");
    assert_eq!(key_after(Some("a0")), "a1");
    assert_eq!(key_after(Some("az")), "b00");
    assert_eq!(key_after(Some("b0z")), "b10");
    let mut k = key_after(None);
    for _ in 0..5000 {
        let n = key_after(Some(&k));
        assert!(n > k, "{n} > {k}");
        k = n;
    }
}

#[test]
fn keys_with_fractions_are_followed_by_the_next_integer() {
    assert_eq!(key_after(Some("a0V")), "a1");
}
