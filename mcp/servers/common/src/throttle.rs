//! The polite brake — a token bucket over every live request to ONE
//! upstream host, shared by this platform's domain MCP servers.
//!
//! Lifted out of `mcp/servers/fedlex/src/backend.rs` at BX, unchanged
//! in behaviour: the fedlex server built it (BS) and the LINDAS server
//! needs the same bucket against a different host. What stayed behind
//! is the WORDING of the refusal — it names a host and a server's own
//! sentence — and what moved here is the mechanism: the bucket, the
//! reservation semantics, the frozen clock for tests and the parser
//! that reads `retry_after_ms` back out of an error text.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------
// BS: the polite brake against the federal endpoint.
// ---------------------------------------------------------------------

/// Default polite brake: two live requests per second sustained …
pub const DEFAULT_UPSTREAM_RATE: f64 = 2.0;
/// … a burst of four …
pub const DEFAULT_UPSTREAM_BURST: f64 = 4.0;
/// … and at most five seconds of waiting before a request is refused
/// as `upstream-busy` instead of queued.
pub const DEFAULT_UPSTREAM_MAX_WAIT: Duration = Duration::from_secs(5);

/// A clock frozen for tests: it advances only when told to, and every
/// sleep the brake asks for is RECORDED instead of taken — so a test
/// reads «this call would have waited 500 ms» in microseconds.
///
/// A recorded sleep does not move the clock. Consecutive `acquire()`
/// calls therefore model requests arriving at the SAME instant —
/// concurrent callers on the shared backend, the only case in which
/// the brake ever refuses — not one client calling in sequence, which
/// on the system clock refills while it sleeps and never waits more
/// than one token's worth. To model a sequential client, `advance`
/// the clock by the wait after each call.
#[derive(Default)]
pub struct FrozenClock {
    now: Mutex<Duration>,
    sleeps: Mutex<Vec<Duration>>,
}

impl FrozenClock {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Lets `by` pass on the frozen clock.
    pub fn advance(&self, by: Duration) {
        *self.now.lock().expect("clock lock not poisoned") += by;
    }

    /// Every sleep the brake asked for, in order.
    pub fn sleeps(&self) -> Vec<Duration> {
        self.sleeps.lock().expect("clock lock not poisoned").clone()
    }

    fn now(&self) -> Duration {
        *self.now.lock().expect("clock lock not poisoned")
    }

    fn sleep(&self, wait: Duration) {
        self.sleeps
            .lock()
            .expect("clock lock not poisoned")
            .push(wait);
    }
}

enum ThrottleClock {
    System { origin: Instant },
    Frozen(Arc<FrozenClock>),
}

/// The brake's answer when a request would have to wait longer than
/// the limit: refused now, reserving nothing, with how long a retry
/// would wait for a free token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UpstreamBusy {
    pub retry_after: Duration,
}

/// The polite brake: a token bucket over EVERY live request to the
/// federal host — SPARQL selects and manifestation fetches share one
/// bucket; cache hits and fixtures never touch it.
///
/// Reservation semantics: a request that finds no token reserves the
/// next one (the bucket goes negative) and WAITS for it — blocking,
/// which is what this synchronous backend does anyway — up to
/// `max_wait`. A request whose wait would exceed that limit reserves
/// nothing and is refused as [`UpstreamBusy`] at once. Admitted
/// requests therefore go in arrival order, and a refused one changes
/// nothing for the others. No persistence: the bucket dies with the
/// process.
pub struct UpstreamThrottle {
    rate_per_second: f64,
    burst: f64,
    max_wait: Duration,
    clock: ThrottleClock,
    state: Mutex<ThrottleState>,
    admitted: AtomicUsize,
    refused: AtomicUsize,
}

struct ThrottleState {
    /// Tokens in the bucket; negative = reservations already handed out.
    tokens: f64,
    /// The clock reading of the last refill.
    last: Duration,
}

impl UpstreamThrottle {
    /// A brake on the system clock. A non-positive rate falls back to
    /// the default, a burst below one token becomes one.
    pub fn new(rate_per_second: f64, burst: f64, max_wait: Duration) -> Self {
        Self::with_clock(
            rate_per_second,
            burst,
            max_wait,
            ThrottleClock::System {
                origin: Instant::now(),
            },
        )
    }

    /// The defaults: 2/s, burst 4, five seconds of patience.
    pub fn default_polite() -> Self {
        Self::new(
            DEFAULT_UPSTREAM_RATE,
            DEFAULT_UPSTREAM_BURST,
            DEFAULT_UPSTREAM_MAX_WAIT,
        )
    }

    /// A brake on a frozen clock (tests): waits are recorded, not taken.
    pub fn frozen(
        rate_per_second: f64,
        burst: f64,
        max_wait: Duration,
        clock: Arc<FrozenClock>,
    ) -> Self {
        Self::with_clock(
            rate_per_second,
            burst,
            max_wait,
            ThrottleClock::Frozen(clock),
        )
    }

    fn with_clock(
        rate_per_second: f64,
        burst: f64,
        max_wait: Duration,
        clock: ThrottleClock,
    ) -> Self {
        let rate_per_second = if rate_per_second.is_finite() && rate_per_second > 0.0 {
            rate_per_second
        } else {
            DEFAULT_UPSTREAM_RATE
        };
        let burst = if burst.is_finite() && burst >= 1.0 {
            burst
        } else {
            1.0
        };
        let last = match &clock {
            ThrottleClock::System { origin } => origin.elapsed(),
            ThrottleClock::Frozen(frozen) => frozen.now(),
        };
        Self {
            rate_per_second,
            burst,
            max_wait,
            clock,
            state: Mutex::new(ThrottleState {
                tokens: burst,
                last,
            }),
            admitted: AtomicUsize::new(0),
            refused: AtomicUsize::new(0),
        }
    }

    pub fn rate_per_second(&self) -> f64 {
        self.rate_per_second
    }

    pub fn burst(&self) -> f64 {
        self.burst
    }

    pub fn max_wait(&self) -> Duration {
        self.max_wait
    }

    /// Requests admitted so far (waited or not).
    pub fn admitted(&self) -> usize {
        self.admitted.load(Ordering::SeqCst)
    }

    /// Requests refused as busy so far.
    pub fn refused(&self) -> usize {
        self.refused.load(Ordering::SeqCst)
    }

    fn now(&self) -> Duration {
        match &self.clock {
            ThrottleClock::System { origin } => origin.elapsed(),
            ThrottleClock::Frozen(frozen) => frozen.now(),
        }
    }

    fn sleep(&self, wait: Duration) {
        match &self.clock {
            ThrottleClock::System { .. } => std::thread::sleep(wait),
            ThrottleClock::Frozen(frozen) => frozen.sleep(wait),
        }
    }

    /// Takes one token, waiting for it when it is not there yet. The
    /// duration waited comes back (zero for a burst token).
    ///
    /// # Errors
    ///
    /// [`UpstreamBusy`] when the wait would exceed `max_wait`; nothing
    /// is reserved in that case.
    pub fn acquire(&self) -> Result<Duration, UpstreamBusy> {
        let wait = {
            let mut state = self.state.lock().expect("throttle lock not poisoned");
            let now = self.now();
            let elapsed = now.saturating_sub(state.last).as_secs_f64();
            state.tokens = (state.tokens + elapsed * self.rate_per_second).min(self.burst);
            state.last = now;
            if state.tokens >= 1.0 {
                state.tokens -= 1.0;
                Duration::ZERO
            } else {
                let deficit = 1.0 - state.tokens;
                let wait = Duration::from_secs_f64(deficit / self.rate_per_second);
                if wait > self.max_wait {
                    self.refused.fetch_add(1, Ordering::SeqCst);
                    return Err(UpstreamBusy { retry_after: wait });
                }
                // The reservation: the bucket goes negative and the
                // next arrival queues behind this one.
                state.tokens -= 1.0;
                wait
            }
        };
        if !wait.is_zero() {
            self.sleep(wait);
        }
        self.admitted.fetch_add(1, Ordering::SeqCst);
        Ok(wait)
    }
}

/// Reads `retry_after_ms` out of an error text the brake raised. The
/// typed `upstream-busy` refusal is built from it — on the hand-written
/// query path (anyhow) and on the vendored one, where the bridge
/// carries the text inside `JoluxError::Transport`.
pub fn busy_retry_after_ms(text: &str) -> Option<u64> {
    let rest = text.split("upstream-busy: retry_after_ms=").nth(1)?;
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}
