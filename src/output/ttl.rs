//! Utility functions for formatting the TTL

use domain::base::Ttl;
use std::fmt::Write as _;

fn chunk(ttl: Ttl) -> (u32, u32, u32, u32) {
    const DAY: u32 = Ttl::DAY.as_secs();
    const HOUR: u32 = Ttl::HOUR.as_secs();
    const MINUTE: u32 = Ttl::MINUTE.as_secs();

    let ttl = ttl.as_secs();
    let (days, ttl) = (ttl / DAY, ttl % DAY);
    let (hours, ttl) = (ttl / HOUR, ttl % HOUR);
    let (minutes, seconds) = (ttl / MINUTE, ttl % MINUTE);
    (days, hours, minutes, seconds)
}

pub fn format(ttl: Ttl) -> String {
    let (days, hours, minutes, seconds) = chunk(ttl);

    let mut s = String::new();

    for (n, unit) in
        [(days, "d"), (hours, "h"), (minutes, "m"), (seconds, "s")]
    {
        if !s.is_empty() {
            write!(s, " {n:>2}{unit}").unwrap();
        } else if n > 0 {
            write!(s, "{n}{unit}").unwrap();
        }
    }

    s
}
