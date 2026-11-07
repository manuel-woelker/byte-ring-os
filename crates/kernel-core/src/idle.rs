//! Deadline policy for a single HTTP connection and the TCP stack.
pub const REQUEST_MS: i64 = 5_000;
pub const CONNECTION_MS: i64 = 15_000;

/// A request deadline applies only until the response is selected. The overall
/// cap remains in effect while sending and closing; a TCP deadline may be sooner.
pub fn deadline(stack: Option<i64>, started: Option<i64>, awaiting_request: bool) -> Option<i64> {
    let application = started.map(|start| {
        start.saturating_add(if awaiting_request {
            REQUEST_MS
        } else {
            CONNECTION_MS
        })
    });
    match (stack, application) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quiet_listener_has_no_tick_and_earliest_deadline_wins() {
        assert_eq!(deadline(None, None, true), None);
        assert_eq!(deadline(Some(400), None, true), Some(400));
        assert_eq!(deadline(None, Some(100), true), Some(5100));
        assert_eq!(deadline(Some(200), Some(100), true), Some(200));
        assert_eq!(deadline(Some(20000), Some(100), true), Some(5100));
        assert_eq!(deadline(None, Some(100), false), Some(15100));
        assert_eq!(deadline(Some(5101), Some(100), false), Some(5101));
        assert_eq!(deadline(None, Some(i64::MAX - 1), false), Some(i64::MAX));
    }
}
