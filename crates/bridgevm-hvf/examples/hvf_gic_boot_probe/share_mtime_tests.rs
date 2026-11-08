use super::*;

#[test]
fn listed_guest_times_become_unix_milliseconds() {
    // Values from Windows PowerShell: [DateTimeOffset]::FromUnixTimeMilliseconds(n).UtcDateTime.ToString('o').
    assert_eq!(guest_mtime_ms("1970-01-01T00:00:00.0000000Z"), Some(0));
    assert_eq!(guest_mtime_ms("2026-11-08T00:05:12.1234567Z"), Some(1_794_096_312_123));
    assert_eq!(guest_mtime_ms("2024-02-29T23:59:59.9990000Z"), Some(1_709_251_199_999));
    assert_eq!(guest_mtime_ms("2000-03-01T00:00:00Z"), Some(951_868_800_000));
    for refused in ["", "2026-11-08T00:05:12", "2026-11-08 00:05:12Z", "1969-12-31T23:59:59.0000000Z",
                    "2026-13-01T00:00:00Z", "2026-11-08T00:05:12.12345678Z", "2026-11-08T00:05:12.+1Z"] {
        assert_eq!(guest_mtime_ms(refused), None, "{refused}");
    }
}
