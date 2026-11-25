//! UUIDv7 identity helpers and a Hybrid Logical Clock (HLC) for total ordering
//! of sync operations without trusting device wall clocks.

use serde::{Deserialize, Serialize};

/// Generate a time-ordered note id with a human-readable prefix.
#[cfg(feature = "native")]
pub fn note_id() -> String {
    format!("note_{}", uuid::Uuid::now_v7().simple())
}

/// Generate an attachment id.
#[cfg(feature = "native")]
pub fn att_id() -> String {
    format!("att_{}", uuid::Uuid::now_v7().simple())
}

/// Generate an operation id.
#[cfg(feature = "native")]
pub fn op_id() -> String {
    format!("op_{}", uuid::Uuid::now_v7().simple())
}

/// A Hybrid Logical Clock timestamp: `(ts, cnt)` where `ts` is millis since the
/// Unix epoch (the maximum of wall clock and last-seen logical time) and `cnt`
/// disambiguates events that share the same millisecond.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Hlc {
    pub ts: u64,
    pub cnt: u32,
}

impl Hlc {
    /// The origin (smallest possible) timestamp.
    pub fn origin() -> Self {
        Hlc { ts: 0, cnt: 0 }
    }

    /// Advance this clock for a new local event at wall time `wall` (millis).
    pub fn tick(&self, wall: u64) -> Self {
        if wall > self.ts {
            Hlc { ts: wall, cnt: 0 }
        } else {
            Hlc {
                ts: self.ts,
                cnt: self.cnt + 1,
            }
        }
    }

    /// Merge with a remote clock and advance for a new local event.
    pub fn merge_tick(&self, remote: &Hlc, wall: u64) -> Self {
        let ts = self.ts.max(remote.ts).max(wall);
        let cnt = if ts == self.ts && ts == remote.ts {
            self.cnt.max(remote.cnt) + 1
        } else if ts == self.ts {
            self.cnt + 1
        } else if ts == remote.ts {
            remote.cnt + 1
        } else {
            0
        };
        Hlc { ts, cnt }
    }

    /// A comparable key. Total order ties are broken by `device_id`.
    pub fn key(&self) -> (u64, u32) {
        (self.ts, self.cnt)
    }
}

/// Total ordering of operations: (hlc.ts, hlc.cnt, device_id).
pub fn op_sort_key(hlc: &Hlc, device_id: &str) -> (u64, u32, String) {
    (hlc.ts, hlc.cnt, device_id.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(feature = "native")]
    fn uuid_v7_ids_have_prefix() {
        assert!(note_id().starts_with("note_"));
        assert!(att_id().starts_with("att_"));
        assert!(op_id().starts_with("op_"));
    }

    #[test]
    fn hlc_ticks_are_monotonic() {
        let a = Hlc::origin().tick(1000);
        let b = a.tick(1000);
        let c = b.tick(1001);
        assert!(a.key() < b.key());
        assert!(b.key() < c.key());
    }

    #[test]
    fn hlc_merge_never_goes_backwards() {
        let local = Hlc { ts: 5000, cnt: 3 };
        let remote = Hlc { ts: 7000, cnt: 1 };
        let merged = local.merge_tick(&remote, 4000);
        assert!(merged.ts >= 7000);
        assert!(merged.key() > local.key());
        assert!(merged.key() > remote.key());
    }
}
