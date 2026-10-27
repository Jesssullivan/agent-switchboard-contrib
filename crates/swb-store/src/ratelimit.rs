//! Per-sender send rate limit (ADR-0003, SWB-R66).
//!
//! One token bucket per sender agent id. A bucket holds at most `burst`
//! sends and refills at `refill_per_minute`. A send that finds its bucket
//! empty is refused with `rate_limited` (HTTP 429). Buckets live in memory
//! only: a broker restart refills every bucket, and nothing here touches the
//! store's tables. Time comes from the store's injected clock, in whole
//! seconds, so tests drive it with a manual clock.

use std::collections::HashMap;

/// Default bucket size: a sender may send this many messages at once.
pub const SEND_BURST_DEFAULT: u32 = 20;
/// Default refill: one send every 2 seconds, sustained.
pub const SEND_REFILL_PER_MINUTE_DEFAULT: u32 = 30;
/// Above this many buckets, an admission first drops every bucket that has
/// refilled to full, which is the same as never having sent.
const SWEEP_ABOVE: usize = 4096;

/// The limit's settings. `burst == 0` turns the limit off.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SendLimit {
    pub burst: u32,
    pub refill_per_minute: u32,
}

impl Default for SendLimit {
    fn default() -> Self {
        Self {
            burst: SEND_BURST_DEFAULT,
            refill_per_minute: SEND_REFILL_PER_MINUTE_DEFAULT,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Bucket {
    tokens: f64,
    at: i64,
}

#[derive(Debug, Default)]
pub(crate) struct Limiter {
    pub(crate) limit: SendLimit,
    buckets: HashMap<String, Bucket>,
}

impl Limiter {
    fn refilled(&self, bucket: Bucket, now: i64) -> f64 {
        let elapsed = (now - bucket.at).max(0) as f64;
        let per_second = f64::from(self.limit.refill_per_minute) / 60.0;
        (bucket.tokens + elapsed * per_second).min(f64::from(self.limit.burst))
    }

    /// Take one send from `sender`'s bucket. `Err(seconds)` is how long
    /// until the bucket holds one again (at least 1).
    pub(crate) fn admit(&mut self, sender: &str, now: i64) -> Result<(), u64> {
        if self.limit.burst == 0 {
            return Ok(());
        }
        if self.buckets.len() > SWEEP_ABOVE {
            let full = f64::from(self.limit.burst);
            let stale: Vec<String> = self
                .buckets
                .iter()
                .filter(|(_, b)| self.refilled(**b, now) >= full)
                .map(|(k, _)| k.clone())
                .collect();
            for key in stale {
                self.buckets.remove(&key);
            }
        }
        let current = self
            .buckets
            .get(sender)
            .map_or(f64::from(self.limit.burst), |bucket| {
                self.refilled(*bucket, now)
            });
        if current >= 1.0 {
            self.buckets.insert(
                sender.to_owned(),
                Bucket {
                    tokens: current - 1.0,
                    at: now,
                },
            );
            return Ok(());
        }
        if self.limit.refill_per_minute == 0 {
            return Err(u64::MAX);
        }
        let per_second = f64::from(self.limit.refill_per_minute) / 60.0;
        Err(((1.0 - current) / per_second).ceil().max(1.0) as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limiter(burst: u32, refill_per_minute: u32) -> Limiter {
        Limiter {
            limit: SendLimit {
                burst,
                refill_per_minute,
            },
            ..Limiter::default()
        }
    }

    #[test]
    fn burst_then_refill_at_the_configured_rate() {
        let mut l = limiter(3, 30);
        for _ in 0..3 {
            assert_eq!(l.admit("a", 100), Ok(()));
        }
        assert_eq!(l.admit("a", 100), Err(2));
        // Another sender has its own bucket.
        assert_eq!(l.admit("b", 100), Ok(()));
        assert_eq!(l.admit("a", 101), Err(1));
        assert_eq!(l.admit("a", 102), Ok(()));
        assert_eq!(l.admit("a", 102), Err(2));
        // A long pause refills to the burst, never past it.
        for _ in 0..3 {
            assert_eq!(l.admit("a", 10_000), Ok(()));
        }
        assert!(l.admit("a", 10_000).is_err());
    }

    #[test]
    fn zero_burst_turns_the_limit_off() {
        let mut l = limiter(0, 0);
        for _ in 0..1000 {
            assert_eq!(l.admit("a", 1), Ok(()));
        }
        assert!(l.buckets.is_empty());
    }

    #[test]
    fn full_buckets_are_swept_when_the_map_grows() {
        let mut l = limiter(2, 60);
        for n in 0..=SWEEP_ABOVE {
            l.admit(&format!("s{n}"), 0).unwrap();
        }
        assert_eq!(l.buckets.len(), SWEEP_ABOVE + 1);
        // Two seconds later every bucket is full again and is dropped.
        l.admit("late", 2).unwrap();
        assert_eq!(l.buckets.len(), 1);
    }
}
