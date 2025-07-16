use code2prompt_tui::ui::token_map_view::format_tokens_integer_arithmetic;

#[test]
fn test_format_tokens_integer_arithmetic() {
    assert_eq!(format_tokens_integer_arithmetic(999), "999");
    assert_eq!(format_tokens_integer_arithmetic(1_000), "1K");
    assert_eq!(format_tokens_integer_arithmetic(1_499), "1K");
    assert_eq!(format_tokens_integer_arithmetic(1_500), "2K");
    assert_eq!(format_tokens_integer_arithmetic(1_501), "2K");
    assert_eq!(format_tokens_integer_arithmetic(1_999), "2K");
    assert_eq!(format_tokens_integer_arithmetic(1_000_000), "1M");
    assert_eq!(format_tokens_integer_arithmetic(2_499_999), "2M");
    assert_eq!(format_tokens_integer_arithmetic(2_500_000), "3M");
}
