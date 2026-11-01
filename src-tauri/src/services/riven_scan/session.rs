//! Scan bookkeeping of a riven roll session (port of the state in WFHelper's `rivenSession`; the
//! window events it also sent belong to the overlay flow).

use std::sync::atomic::{AtomicU64, Ordering};

/// Hands out scan ids so a scan that finishes after a newer one began (or after a close) is dropped.
#[derive(Default)]
pub struct ScanGeneration(AtomicU64);

impl ScanGeneration {
    pub const fn new() -> Self {
        Self(AtomicU64::new(0))
    }

    pub fn begin(&self) -> u64 {
        self.0.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub fn invalidate(&self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }

    pub fn is_current(&self, generation: u64) -> bool {
        self.0.load(Ordering::SeqCst) == generation
    }

    pub fn current(&self) -> u64 {
        self.0.load(Ordering::SeqCst)
    }
}

/// Rolls done and Kuva spent since the session started.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RollTally {
    pub kuva_per_roll: u32,
    pub roll_count: u32,
    pub total_kuva_spent: u64,
}

impl RollTally {
    pub fn new(kuva_per_roll: u32) -> Self {
        Self {
            kuva_per_roll,
            ..Self::default()
        }
    }

    /// The cost of the cycle the game now asks to confirm; it climbs per cycle. 0 (unparsed) keeps the last.
    pub fn set_cost(&mut self, kuva_per_roll: u32) {
        if kuva_per_roll > 0 {
            self.kuva_per_roll = kuva_per_roll;
        }
    }

    /// Counts a completed roll, whether or not its scan read anything.
    pub fn roll_done(&mut self) -> Self {
        self.roll_count += 1;
        self.total_kuva_spent += u64::from(self.kuva_per_roll);
        *self
    }
}

/// Kuva cost of the n-th cycle (1-based); it climbs per roll and stays at the cap from the 9th.
const CYCLE_COSTS: [u64; 9] = [900, 1000, 1200, 1400, 1700, 2000, 2350, 2750, 3500];

/// Kuva a riven has cost after `rolls` cycles.
pub fn kuva_spent(rolls: u32) -> u64 {
    (0..rolls as usize)
        .map(|i| CYCLE_COSTS[i.min(CYCLE_COSTS.len() - 1)])
        .sum()
}

#[cfg(test)]
mod tests {
    use super::{RollTally, kuva_spent};

    #[test]
    fn each_roll_adds_the_cost_its_dialog_asked() {
        let mut t = RollTally::new(900);
        t.roll_done();
        t.set_cost(1000);
        t.set_cost(0);
        assert_eq!(t.roll_done().total_kuva_spent, 1900);
    }

    #[test]
    fn kuva_climbs_then_caps() {
        assert_eq!(kuva_spent(0), 0);
        assert_eq!(kuva_spent(1), 900);
        assert_eq!(kuva_spent(9), 16_800);
        assert_eq!(kuva_spent(39), 16_800 + 30 * 3500);
    }
}
