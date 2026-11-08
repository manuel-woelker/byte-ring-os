//! Two-slot round robin with bounded quanta and per-process CPU budgets.
pub const CAPACITY: usize = 2;
pub const QUANTUM_MS: u64 = 10;
pub const BUDGET_MS: u64 = 1000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Decision {
    Continue,
    Rotate { from: usize, to: usize },
    BudgetExpired(usize),
    Finished,
}

pub struct Scheduler {
    runnable: [bool; CAPACITY],
    remaining: [u64; CAPACITY],
    current: Option<usize>,
    accounted: i64,
    deadline: i64,
}
impl Scheduler {
    pub fn new(count: usize, now: i64) -> Self {
        assert!(count <= CAPACITY);
        let mut runnable = [false; CAPACITY];
        runnable[..count].fill(true);
        Self {
            runnable,
            remaining: [BUDGET_MS; CAPACITY],
            current: (count != 0).then_some(0),
            accounted: now,
            deadline: now.saturating_add(QUANTUM_MS as i64),
        }
    }
    pub fn current(&self) -> Option<usize> {
        self.current
    }
    fn account(&mut self, now: i64) {
        let now = now.max(self.accounted);
        if let Some(current) = self.current {
            self.remaining[current] =
                self.remaining[current].saturating_sub(now.saturating_sub(self.accounted) as u64);
        }
        self.accounted = now;
    }
    fn select(&mut self, after: usize, now: i64) -> Option<usize> {
        self.current = (1..=CAPACITY)
            .map(|step| (after + step) % CAPACITY)
            .find(|&slot| self.runnable[slot]);
        if let Some(next) = self.current {
            self.deadline = now.saturating_add(QUANTUM_MS.min(self.remaining[next]) as i64);
        }
        self.accounted = now;
        self.current
    }
    /// Early/stale timer delivery never shortens the current quantum.
    pub fn tick(&mut self, now: i64) -> Decision {
        let Some(current) = self.current else {
            return Decision::Finished;
        };
        self.account(now);
        if self.remaining[current] == 0 {
            return Decision::BudgetExpired(current);
        }
        if now < self.deadline {
            return Decision::Continue;
        }
        let next = self
            .select(current, self.accounted)
            .expect("current slot remains runnable");
        Decision::Rotate {
            from: current,
            to: next,
        }
    }
    /// Retiring a slot never resurrects it; a later batch needs a new scheduler.
    pub fn retire(&mut self, now: i64) -> Option<usize> {
        let current = self.current?;
        self.account(now);
        self.runnable[current] = false;
        self.select(current, self.accounted)
    }
    pub fn delay(&self, now: i64) -> Option<u64> {
        self.current
            .map(|_| self.deadline.saturating_sub(now).max(0) as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn round_robin_and_stale_delivery_preserve_quanta() {
        let mut s = Scheduler::new(2, 0);
        assert_eq!(s.current(), Some(0));
        assert_eq!(s.tick(4), Decision::Continue);
        assert_eq!(s.delay(4), Some(6));
        assert_eq!(s.tick(10), Decision::Rotate { from: 0, to: 1 });
        assert_eq!(s.tick(10), Decision::Continue);
        assert_eq!(s.delay(11), Some(9));
        assert_eq!(s.tick(20), Decision::Rotate { from: 1, to: 0 });
    }
    #[test]
    fn budget_accounts_cpu_time_without_charging_the_waiting_peer() {
        let mut s = Scheduler::new(2, 0);
        assert_eq!(s.tick(10), Decision::Rotate { from: 0, to: 1 });
        assert_eq!(s.retire(15), Some(0));
        assert_eq!(s.tick(1004), Decision::Rotate { from: 0, to: 0 });
        assert_eq!(s.delay(1004), Some(1));
        assert_eq!(s.tick(1005), Decision::BudgetExpired(0));
        assert_eq!(s.retire(1005), None);
        assert_eq!(s.tick(2000), Decision::Finished);
    }
    #[test]
    fn retired_slots_are_skipped_and_an_empty_batch_stays_finished() {
        let mut s = Scheduler::new(2, 0);
        assert_eq!(s.retire(1), Some(1));
        for time in [11, 21, 31] {
            assert_eq!(s.tick(time), Decision::Rotate { from: 1, to: 1 });
        }
        assert_eq!(s.retire(32), None);
        assert_eq!(s.retire(33), None);
        assert_eq!(s.delay(33), None);
        assert_eq!(Scheduler::new(0, 0).current(), None);
    }
}
