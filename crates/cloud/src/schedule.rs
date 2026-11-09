//! When Sync steps run: the pure schedule of the Sync loop and the status the
//! user sees. Times are offsets from any fixed start, so a client that drives
//! time itself (the web app's timers) and a test are their own clock. No I/O,
//! no clock of its own (ADR-0016).

use std::time::Duration;

use crate::step::StepOutcome;

/// How long the loop waits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timing {
    /// Quiet time after the last local change before a step runs.
    pub debounce: Duration,
    /// Time between steps while in front and nothing happens.
    pub poll: Duration,
    /// The longest wait between steps while they fail.
    pub max_backoff: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Self {
            debounce: Duration::from_secs(3),
            poll: Duration::from_secs(10),
            max_backoff: Duration::from_secs(60),
        }
    }
}

/// When the next step is due, as a pure function of what happened and when.
/// Times are offsets from any fixed start, so a test is its own clock.
#[derive(Debug)]
pub struct Schedule {
    timing: Timing,
    in_front: bool,
    /// Whether the Device has a connection.
    online: bool,
    /// Whether the cloud rejected the credentials: nothing runs until the
    /// user Reconnects.
    blocked: bool,
    /// When the next step runs; none while nothing is scheduled.
    next: Option<Duration>,
    /// Steps that failed in a row.
    failures: u32,
}

impl Schedule {
    /// In front, with a step due at once.
    pub fn new(timing: Timing, now: Duration) -> Self {
        Self {
            timing,
            in_front: true,
            online: true,
            blocked: false,
            next: Some(now),
            failures: 0,
        }
    }

    /// Whether the cloud rejected the credentials and nothing runs until
    /// `reconnected`.
    pub fn blocked(&self) -> bool {
        self.blocked
    }

    /// Whether steps may run at all.
    fn active(&self) -> bool {
        self.in_front && self.online && !self.blocked
    }

    pub fn came_to_front(&mut self, now: Duration) {
        self.in_front = true;
        self.next = self.active().then_some(now);
    }

    pub fn went_to_background(&mut self) {
        self.in_front = false;
        self.next = None;
    }

    /// Every change pushes the step back, so a burst becomes one step.
    pub fn local_change(&mut self, now: Duration) {
        if self.active() {
            self.next = Some(now + self.timing.debounce);
        }
    }

    /// The connection is gone or back. Back syncs at once, and starts the
    /// back-off over.
    pub fn set_online(&mut self, online: bool, now: Duration) {
        self.online = online;
        if online {
            self.failures = 0;
            self.next = self.active().then_some(now);
        } else {
            self.next = None;
        }
    }

    /// The cloud rejected the credentials.
    pub fn access_lost(&mut self) {
        self.blocked = true;
        self.next = None;
    }

    /// The user Reconnected: carries on at once.
    pub fn reconnected(&mut self, now: Duration) {
        self.blocked = false;
        self.failures = 0;
        self.next = self.active().then_some(now);
    }

    /// How long until the next step; none if there won't be one.
    pub fn wait(&self, now: Duration) -> Option<Duration> {
        self.next.map(|next| next.saturating_sub(now))
    }

    /// Whether a step runs now. If so, the schedule counts it as running.
    pub fn take_due(&mut self, now: Duration) -> bool {
        match self.next {
            Some(next) if next <= now => {
                self.next = None;
                true
            }
            _ => false,
        }
    }

    /// A step ended at `now`. A change or a coming to the front during the
    /// step keeps its earlier time.
    pub fn step_ended(&mut self, now: Duration, succeeded: bool) {
        self.failures = if succeeded { 0 } else { self.failures + 1 };
        if !self.active() {
            return;
        }
        let wait = if self.failures == 0 {
            self.timing.poll
        } else {
            // Doubles with every failure.
            let doubled = self.timing.poll.saturating_mul(1 << self.failures.min(16));
            doubled.min(self.timing.max_backoff.max(self.timing.poll))
        };
        let planned = now + wait;
        self.next = Some(self.next.map_or(planned, |next| next.min(planned)));
    }
}

/// What Cloud Backing is doing, for the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// The last step went through (or none had to transfer anything).
    UpToDate,
    /// Drawings are being transferred.
    Syncing,
    /// No connection, or the cloud can't be reached; Sync retries.
    Offline,
    /// The cloud rejected the credentials; Sync waits for a Reconnect.
    AccessLost,
    /// The cloud has no space left: nothing is uploaded, downloads go on.
    Full,
}

impl Status {
    /// The status after a step: a lost access beats being offline, which beats
    /// a full cloud.
    pub fn after(outcome: &StepOutcome) -> Self {
        Self::after_all(std::slice::from_ref(outcome))
    }

    /// The status after the steps of all Context folders together.
    pub fn after_all(outcomes: &[StepOutcome]) -> Self {
        if outcomes.iter().any(|o| o.access_lost) {
            Status::AccessLost
        } else if outcomes.iter().any(|o| o.error.is_some()) {
            Status::Offline
        } else if outcomes.iter().any(|o| o.storage_full) {
            Status::Full
        } else {
            Status::UpToDate
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secs(s: u64) -> Duration {
        Duration::from_secs(s)
    }

    /// Whether a step runs at `now`, counting it as run (and succeeded) if so.
    fn runs(schedule: &mut Schedule, now: u64) -> bool {
        let due = schedule.take_due(secs(now));
        if due {
            schedule.step_ended(secs(now), true);
        }
        due
    }

    fn schedule() -> Schedule {
        Schedule::new(Timing::default(), secs(0))
    }

    #[test]
    fn syncs_at_once_on_start_and_then_polls_every_10_seconds() {
        let mut s = schedule();
        assert!(runs(&mut s, 0));
        assert!(!runs(&mut s, 9));
        assert!(runs(&mut s, 10));
        assert!(!runs(&mut s, 19));
        assert!(runs(&mut s, 20));
    }

    #[test]
    fn coming_to_front_syncs_at_once() {
        let mut s = schedule();
        runs(&mut s, 0);
        s.went_to_background();
        s.came_to_front(secs(4));
        assert!(runs(&mut s, 4));
    }

    #[test]
    fn nothing_runs_in_the_background() {
        let mut s = schedule();
        runs(&mut s, 0);
        s.went_to_background();
        s.local_change(secs(5));
        assert_eq!(s.wait(secs(5)), None);
        assert!(!runs(&mut s, 1000));
    }

    #[test]
    fn a_burst_of_local_changes_is_one_step_after_the_debounce() {
        let mut s = schedule();
        runs(&mut s, 0);
        for t in [1, 2, 3, 4] {
            s.local_change(secs(t));
        }
        assert!(!runs(&mut s, 6)); // 3 s after the first change, but not the last
        assert!(runs(&mut s, 7));
        assert!(!runs(&mut s, 8));
    }

    #[test]
    fn a_change_during_a_step_is_not_lost() {
        let mut s = schedule();
        assert!(s.take_due(secs(0)));
        s.local_change(secs(1));
        s.step_ended(secs(2), true);
        assert!(runs(&mut s, 4));
    }

    #[test]
    fn failing_steps_back_off_up_to_a_limit_and_success_resets_it() {
        let mut s = schedule();
        let mut now = 0;
        let mut waits = vec![];
        for _ in 0..5 {
            assert!(s.take_due(secs(now)));
            s.step_ended(secs(now), false);
            let wait = s.wait(secs(now)).unwrap().as_secs();
            waits.push(wait);
            now += wait;
        }
        assert_eq!(waits, [20, 40, 60, 60, 60]);
        assert!(s.take_due(secs(now)));
        s.step_ended(secs(now), true);
        assert_eq!(s.wait(secs(now)), Some(secs(10)));
    }

    #[test]
    fn offline_no_step_runs_and_coming_online_syncs_at_once() {
        let mut s = schedule();
        runs(&mut s, 0);
        s.set_online(false, secs(3));
        s.local_change(secs(4));
        assert!(!runs(&mut s, 10));
        assert!(!runs(&mut s, 1000));
        s.set_online(true, secs(1001));
        assert!(runs(&mut s, 1001));
    }

    #[test]
    fn coming_online_in_the_background_waits_for_the_front() {
        let mut s = schedule();
        s.went_to_background();
        s.set_online(false, secs(1));
        s.set_online(true, secs(2));
        assert!(!runs(&mut s, 2));
        s.came_to_front(secs(3));
        assert!(runs(&mut s, 3));
    }

    #[test]
    fn coming_online_starts_the_back_off_over() {
        let mut s = schedule();
        for now in [0, 20, 60] {
            assert!(s.take_due(secs(now)));
            s.step_ended(secs(now), false);
        }
        s.set_online(false, secs(61));
        s.set_online(true, secs(62));
        assert!(s.take_due(secs(62)));
        s.step_ended(secs(62), false);
        assert_eq!(s.wait(secs(62)), Some(secs(20)));
    }

    #[test]
    fn lost_access_blocks_every_trigger_until_reconnected() {
        let mut s = schedule();
        assert!(s.take_due(secs(0)));
        s.step_ended(secs(0), true);
        s.access_lost();
        s.local_change(secs(1));
        s.came_to_front(secs(2));
        s.set_online(true, secs(3));
        assert_eq!(s.wait(secs(3)), None);
        assert!(!runs(&mut s, 1000));
        s.reconnected(secs(1001));
        assert!(runs(&mut s, 1001));
    }

    #[test]
    fn rejected_credentials_block_again_after_the_reconnect_step() {
        let mut s = schedule();
        assert!(s.take_due(secs(0)));
        s.access_lost();
        s.reconnected(secs(5));
        assert!(s.take_due(secs(5)));
        s.step_ended(secs(5), true);
        s.access_lost();
        assert_eq!(s.wait(secs(6)), None);
    }

    #[test]
    fn the_status_covers_all_folders_the_worst_problem_winning() {
        let ok = StepOutcome::default;
        let full = || StepOutcome {
            storage_full: true,
            ..ok()
        };
        let lost = || StepOutcome {
            access_lost: true,
            ..ok()
        };
        assert_eq!(Status::after_all(&[ok(), ok()]), Status::UpToDate);
        assert_eq!(Status::after_all(&[ok(), full()]), Status::Full);
        assert_eq!(Status::after_all(&[full(), lost()]), Status::AccessLost);
    }
}
