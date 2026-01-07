use time::OffsetDateTime;

/// Returns the current UTC timestamp in milliseconds, clamping to i64::MAX on overflow.
pub fn now_ms() -> i64 {
    let nanos = OffsetDateTime::now_utc().unix_timestamp_nanos() / 1_000_000;
    i64::try_from(nanos).unwrap_or(i64::MAX)
}
