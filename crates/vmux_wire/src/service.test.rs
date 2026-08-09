use super::*;

#[test]
fn format_mem_buckets() {
    assert_eq!(format_mem(0), "—");
    assert_eq!(format_mem(512 * 1024), "<1 MB");
    assert_eq!(format_mem(332 * 1024 * 1024), "332 MB");
    assert_eq!(format_mem(3 * 1024 * 1024 * 1024 / 2), "1.5 GB");
}
