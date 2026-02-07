/// Keepalive timer: 10s idle → send REQUEST, 3 missed → disconnect.
use crate::constants::{KEEPALIVE_INTERVAL_SECS, KEEPALIVE_MAX_MISSED};
use tokio::time::{Duration, Instant};

pub struct KeepaliveTracker {
    /// Counter for the REQUEST payload ("REQUEST N").
    request_counter: u32,
    /// Number of requests sent without receiving a reply.
    no_reply_count: u32,
    /// When we last saw any data from the TUN device (used to trigger keepalive).
    last_tun_activity: Instant,
    /// When we last received any data from the SSL socket.
    last_socket_activity: Instant,
}

impl KeepaliveTracker {
    pub fn new() -> Self {
        let now = Instant::now();
        KeepaliveTracker {
            request_counter: 0,
            no_reply_count: 0,
            last_tun_activity: now,
            last_socket_activity: now,
        }
    }

    /// Record that we received data from the TUN device.
    pub fn mark_tun_activity(&mut self) {
        self.last_tun_activity = Instant::now();
    }

    /// Record that we received data from the SSL socket.
    pub fn mark_socket_activity(&mut self) {
        self.last_socket_activity = Instant::now();
    }

    /// Record that we received a keepalive REPLY.
    pub fn received_reply(&mut self) {
        self.no_reply_count = 0;
    }

    /// Check if a keepalive request should be sent.
    /// Returns the request counter if yes.
    pub fn should_send_request(&mut self) -> Option<u32> {
        let idle_duration = Duration::from_secs(KEEPALIVE_INTERVAL_SECS);
        if self.last_tun_activity.elapsed() >= idle_duration {
            let counter = self.request_counter;
            self.request_counter += 1;
            self.no_reply_count += 1;
            self.last_tun_activity = Instant::now(); // Reset to avoid sending every tick
            Some(counter)
        } else {
            None
        }
    }

    /// Check if the connection should be considered dead.
    pub fn is_dead(&self) -> bool {
        self.no_reply_count > KEEPALIVE_MAX_MISSED
    }

    /// Get the duration until the next keepalive check.
    pub fn next_check_duration(&self) -> Duration {
        let elapsed = self.last_tun_activity.elapsed();
        let interval = Duration::from_secs(KEEPALIVE_INTERVAL_SECS);
        if elapsed >= interval {
            Duration::from_millis(100) // Check soon
        } else {
            interval - elapsed
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_tracker_not_dead() {
        let tracker = KeepaliveTracker::new();
        assert!(!tracker.is_dead());
    }

    #[test]
    fn test_reply_resets_count() {
        let mut tracker = KeepaliveTracker::new();
        tracker.no_reply_count = 3;
        tracker.received_reply();
        assert_eq!(tracker.no_reply_count, 0);
        assert!(!tracker.is_dead());
    }

    #[test]
    fn test_dead_after_max_missed() {
        let mut tracker = KeepaliveTracker::new();
        tracker.no_reply_count = KEEPALIVE_MAX_MISSED + 1;
        assert!(tracker.is_dead());
    }
}
