//! Clock sync by ping/pong against the host, keeping the offset of the lowest round trip
//! (as main.gd). Times are seconds on the local monotonic clock.
#[derive(Default, Clone, Copy, Debug)]
pub struct ClockSync {
    pub offset: f64,
    pub best_rtt: Option<f64>,
    pub ready: bool,
}

impl ClockSync {
    pub fn host() -> ClockSync {
        ClockSync { offset: 0.0, best_rtt: None, ready: true }
    }
    /// `sent` is the local time the ping left, `now` the local time the pong arrived,
    /// `server_time` the host's clock when it answered.
    pub fn on_pong(&mut self, sent: f64, server_time: f64, now: f64) {
        let rtt = now - sent;
        if self.best_rtt.is_none_or(|b| rtt < b) {
            self.best_rtt = Some(rtt);
            self.offset = server_time - (sent + now) * 0.5;
        }
        self.ready = true;
    }
    pub fn server_now(&self, local_now: f64) -> f64 {
        local_now + self.offset
    }
}
