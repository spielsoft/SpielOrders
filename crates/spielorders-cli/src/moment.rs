//! The one place a moment in time is rendered for a human.
//!
//! A stored Date Ordered needs no renderer: it is a calendar day the user wrote and it
//! reads back as they wrote it. A filesystem timestamp is different — it arrives as a
//! count of seconds, which is a fact about the epoch rather than about the file, and a
//! reader asking when their file changed cannot use it.
//!
//! Rendered in the machine's own zone, because the question a reader is asking is when
//! this happened where they are. The machine-readable response keeps the number: a
//! consumer converts it to whatever zone it is presenting in, and a rendering would take
//! that choice away from it.
//!
//! Decision sources: Define the Rust CLI capability contract.

/// One moment, as a human reads it: the local date and time, to the second.
///
/// Seconds are shown because two files written in the same minute are ordinary and a
/// reader comparing them has nothing else to go on here — the fallback ordering is by
/// name, not by time.
///
/// A count of seconds no calendar can place reads back as itself. That is not reachable
/// from a filesystem stamp on any machine this runs on, and inventing a date for it would
/// say something false about a file; showing the number says only that the platform
/// reported something unusable.
pub(crate) fn displayed_moment(unix_seconds: i64) -> String {
    jiff::Timestamp::from_second(unix_seconds)
        .map(|moment| {
            moment
                .to_zoned(jiff::tz::TimeZone::system())
                .strftime("%Y-%m-%d %H:%M:%S")
                .to_string()
        })
        .unwrap_or_else(|_| unix_seconds.to_string())
}
