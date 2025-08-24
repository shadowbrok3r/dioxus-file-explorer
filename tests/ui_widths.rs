use dioxus_file_explorer::app::normalize_detail_widths;

#[test]
fn normalize_scales_sum_close_to_target() {
    let mut widths = [2.0, 3.0, 1.0, 2.0, 1.0, 0.5];
    normalize_detail_widths(&mut widths);
    let sum: f32 = widths.iter().sum();
    assert!((sum - 7.2).abs() < 0.5, "sum after normalize too far from target: {sum}");
    assert!(widths.iter().all(|w| *w >= 0.35 && *w <= 6.0));
}

#[test]
fn normalize_handles_tiny_values() {
    let mut widths = [0.01, 0.02, 0.03, 0.04, 0.05, 0.06];
    normalize_detail_widths(&mut widths);
    for w in widths.iter() { assert!(*w >= 0.35, "width not clamped up: {w}"); }
}
