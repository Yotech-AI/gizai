use gizai_core::projects::suggest_key;

#[test]
fn keys_come_from_initials_or_the_first_letters() {
    assert_eq!(suggest_key("Kade Logistics portal"), "KLP");
    assert_eq!(suggest_key("Kade portal"), "KP");
    assert_eq!(suggest_key("Webshop"), "WEBS");
    assert_eq!(suggest_key("2026 rebrand"), "P2R");
    assert_eq!(suggest_key("X"), "XP");
    assert_eq!(suggest_key("  "), "PRJ");
    assert_eq!(suggest_key("a b c d e f g h"), "ABCDEF");
}
