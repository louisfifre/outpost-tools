// The PC's memory and the amounts a profile may ask for (play-a-profile.md, Profile page, Memory). Amounts of a
// profile are in MiB, the unit of Prism's MinMemAlloc and MaxMemAlloc in instance.cfg (see profiles.rs); the PC's
// own figures are in bytes. A "gigabyte" here is 1024 MiB, which is what Java's -Xmx counts.
// The amounts below (clamp, plain_default) wait for New profile, not built yet; only PcMemory::read is wired today.
// When they are wired, clamp must step by half gigabytes, as the profile page's memory bar does.
#![allow(dead_code)]

use sysinfo::{MemoryRefreshKind, System};

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
        // The RAM only: the swap is not memory a game can count on.
        system.refresh_memory_specifics(MemoryRefreshKind::nothing().with_ram());
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
    mb.saturating_add(GB_MB / 2).div_euclid(GB_MB)
}

/// Less than one whole gigabyte is free: no amount can be offered, and the interface says so.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NotEnoughFree {
    pub free_mb: i64,
}

// Whole gigabytes, in MiB. The maximum is at least 1 GB (profiles.rs refuses less) and never above the whole
// gigabytes that are free; the minimum is at least 1 GB and never above the maximum. With less than 1 GB free there
// is nothing to offer: `NotEnoughFree`, never a 1 GB maximum above what is free.
pub fn clamp(min_mb: i64, max_mb: i64, free_bytes: u64) -> Result<Amounts, NotEnoughFree> {
    let free_gb = i64::try_from(free_bytes / GB_BYTES).unwrap_or(i64::MAX);
    if free_gb < 1 {
        return Err(NotEnoughFree { free_mb: i64::try_from(free_bytes / (1024 * 1024)).unwrap_or(i64::MAX) });
    }
    let max_gb = to_gb(max_mb).clamp(1, free_gb);
    let min_gb = to_gb(min_mb).clamp(1, max_gb);
    Ok(Amounts { min_mb: min_gb.saturating_mul(GB_MB), max_mb: max_gb.saturating_mul(GB_MB) })
}

// What a new plain-game profile starts with, held to what is free.
pub fn plain_default(free_bytes: u64) -> Result<Amounts, NotEnoughFree> {
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

    fn amounts(min_mb: i64, max_mb: i64) -> Result<Amounts, NotEnoughFree> {
        Ok(Amounts { min_mb, max_mb })
    }

    #[test]
    fn rounds_to_whole_gigabytes() {
        assert_eq!(clamp(1500, 3000, gb(16)), amounts(1024, 3072));
        assert_eq!(clamp(2048, 4096, gb(16)), amounts(2048, 4096));
    }

    #[test]
    fn maximum_stays_in_what_is_free() {
        assert_eq!(clamp(1024, 16384, gb(6)).unwrap().max_mb, 6144);
        assert_eq!(clamp(1024, 16384, gb(6) + GB_BYTES / 2).unwrap().max_mb, 6144);
        // Exactly one free gigabyte is enough for a 1 GB maximum.
        assert_eq!(clamp(1024, 16384, gb(1)), amounts(1024, 1024));
    }

    #[test]
    fn minimum_stays_under_maximum() {
        assert_eq!(clamp(8192, 2048, gb(16)), amounts(2048, 2048));
        assert_eq!(clamp(8192, 8192, gb(3)), amounts(3072, 3072));
    }

    #[test]
    fn less_than_a_gigabyte_free_is_said_not_rounded_up() {
        assert_eq!(clamp(0, 0, 0), Err(NotEnoughFree { free_mb: 0 }));
        assert_eq!(clamp(1024, 2048, gb(1) / 2), Err(NotEnoughFree { free_mb: 512 }));
        assert_eq!(clamp(1024, 2048, gb(1) - 1), Err(NotEnoughFree { free_mb: 1023 }));
    }

    #[test]
    fn nothing_overflows() {
        assert_eq!(to_gb(i64::MAX), i64::MAX / GB_MB);
        assert_eq!(clamp(i64::MAX, i64::MAX, u64::MAX), amounts(((1i64 << 34) - 1) * GB_MB, ((1i64 << 34) - 1) * GB_MB));
        assert_eq!(clamp(i64::MIN, i64::MIN, gb(4)), amounts(1024, 1024));
    }

    #[test]
    fn plain_default_is_two_gigabytes() {
        assert_eq!(plain_default(gb(16)), amounts(1024, 2048));
        assert_eq!(plain_default(gb(1)), amounts(1024, 1024));
        assert!(plain_default(gb(1) / 2).is_err());
    }
}
