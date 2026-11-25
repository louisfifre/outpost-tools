// The PC's memory and the amounts a profile may ask for (play-a-profile.md, Profile page, Memory). Amounts of a
// profile are in MiB, the unit of Prism's MinMemAlloc and MaxMemAlloc in instance.cfg (see profiles.rs); the PC's
// own figures are in bytes. A "gigabyte" here is 1024 MiB, which is what Java's -Xmx counts.

use sysinfo::System;

pub const GB_BYTES: u64 = 1024 * 1024 * 1024;
const GB_MB: i64 = 1024;

// The plain game's maximum, decided with the maintainer (2026-11-25).
pub const PLAIN_MAX_GB: i64 = 2;
// To validate by the maintainer: the starting minimum of a new profile.
pub const START_MIN_GB: i64 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PcMemory {
    pub total: u64,
    pub used: u64,
}

impl PcMemory {
    pub fn read() -> PcMemory {
        let mut system = System::new();
        system.refresh_memory();
        PcMemory { total: system.total_memory(), used: system.used_memory() }
    }

    pub fn free(&self) -> u64 {
        self.total.saturating_sub(self.used)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Amounts {
    pub min_mb: i64,
    pub max_mb: i64,
}

fn to_gb(mb: i64) -> i64 {
    (mb + GB_MB / 2).div_euclid(GB_MB)
}

// Whole gigabytes, in MiB. The maximum is at least 1 GB (profiles.rs refuses less) and never above the whole
// gigabytes that are free, except that one gigabyte is always kept; the minimum is at least 1 GB and never above
// the maximum.
pub fn clamp(min_mb: i64, max_mb: i64, free_bytes: u64) -> Amounts {
    let free_gb = i64::try_from(free_bytes / GB_BYTES).unwrap_or(i64::MAX);
    let max_gb = to_gb(max_mb).clamp(1, free_gb.max(1));
    let min_gb = to_gb(min_mb).clamp(1, max_gb);
    Amounts { min_mb: min_gb * GB_MB, max_mb: max_gb * GB_MB }
}

// What a new plain-game profile starts with, held to what is free.
pub fn plain_default(free_bytes: u64) -> Amounts {
    clamp(START_MIN_GB * GB_MB, PLAIN_MAX_GB * GB_MB, free_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gb(n: u64) -> u64 {
        n * GB_BYTES
    }

    #[test]
    fn reads_the_pc() {
        let pc = PcMemory::read();
        assert!(pc.total > 0);
        assert!(pc.used <= pc.total);
        assert_eq!(pc.free(), pc.total - pc.used);
    }

    #[test]
    fn free_never_goes_negative() {
        assert_eq!(PcMemory { total: 1, used: 5 }.free(), 0);
    }

    #[test]
    fn rounds_to_whole_gigabytes() {
        assert_eq!(clamp(1500, 3000, gb(16)), Amounts { min_mb: 1024, max_mb: 3072 });
        assert_eq!(clamp(2048, 4096, gb(16)), Amounts { min_mb: 2048, max_mb: 4096 });
    }

    #[test]
    fn maximum_stays_in_what_is_free() {
        assert_eq!(clamp(1024, 16384, gb(6)).max_mb, 6144);
        assert_eq!(clamp(1024, 16384, gb(6) + GB_BYTES / 2).max_mb, 6144);
    }

    #[test]
    fn minimum_stays_under_maximum() {
        assert_eq!(clamp(8192, 2048, gb(16)), Amounts { min_mb: 2048, max_mb: 2048 });
        assert_eq!(clamp(8192, 8192, gb(3)), Amounts { min_mb: 3072, max_mb: 3072 });
    }

    #[test]
    fn keeps_one_gigabyte_when_nothing_is_free() {
        assert_eq!(clamp(0, 0, 0), Amounts { min_mb: 1024, max_mb: 1024 });
        assert_eq!(clamp(-5, -5, gb(1) / 2), Amounts { min_mb: 1024, max_mb: 1024 });
    }

    #[test]
    fn plain_default_is_two_gigabytes() {
        assert_eq!(plain_default(gb(16)), Amounts { min_mb: 1024, max_mb: 2048 });
        assert_eq!(plain_default(gb(1)), Amounts { min_mb: 1024, max_mb: 1024 });
    }
}
