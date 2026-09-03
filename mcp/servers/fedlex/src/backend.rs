//! SPARQL backend abstraction: the domain logic never knows whether
//! it talks to the live public Fedlex endpoint or to recorded
//! fixtures. Fixture keys are SEMANTIC (tool + params), never raw
//! query bytes — refactoring a query re-records deliberately instead
//! of silently missing the fixture.
//!
//! BQ added the bridge onto the vendored `fedlex-jolux` primitives
//! ([`KeyedClient`]): those primitives are written against an async
//! `SparqlClient` trait, and this backend is synchronous by design
//! (one polite request at a time, no runtime in the library). The
//! bridge answers every `query` synchronously and keeps the semantic
//! fixture key per tool call, so a vendored primitive runs offline on
//! the same recorded reality as the hand-written v0 queries.
//!
//! BO′ added the bounded in-process manifestation cache
//! ([`ManifestationCache`]) for the LIVE backend: a research loop that
//! reads five articles of one act fetched the same 433 KB file five
//! times — not «single polite requests». The cache keeps the fetched
//! manifestation (URL + body + the moment it was really retrieved)
//! under the SAME semantic key the fixture uses, LRU-evicted by bytes
//! and entries, never persisted, never consulted by the Fixtures or
//! Recording backends. Honesty rule (TOOLSET-v0: «cache serving is
//! marked in transaction_time semantics, not hidden»): a cache hit
//! carries the ORIGINAL retrieval time and says `served: cache`.
//!
//! BS added the polite brake ([`UpstreamThrottle`]): every live request
//! to the federal host — SPARQL selects and manifestation fetches
//! alike, in the Live and Recording backends — takes a token from one
//! bucket (two a second sustained, a burst of four by default). A
//! request that finds no token reserves the next one and WAITS for it,
//! blocking, up to five seconds; beyond that it is refused at once as
//! the typed `upstream-busy` with the moment a retry would find a
//! token. Cache hits and fixtures never touch the bucket. «Single
//! polite requests, no campaigns» is thereby a property of the code,
//! not only a sentence in the manifest.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{bail, Context as _, Result};

/// The public Fedlex SPARQL endpoint (ground truth verified in the
/// S3 spec run and re-verified at build time; a public federal
/// endpoint used normally — single queries, no campaigns).
pub const FEDLEX_ENDPOINT: &str = "https://fedlex.data.admin.ch/sparqlendpoint";

/// The only host manifestations (Akoma-Ntoso XML) are fetched from.
///
/// Recorded reality (BGÖ, KVG, LSV manifestations at BQ): every
/// `jolux:isExemplifiedBy` URL the graph hands out lives under
/// `https://fedlex.data.admin.ch/filestore/…` — the SAME host as the
/// SPARQL endpoint, not `www.fedlex.admin.ch` (the human portal).
/// [`Backend::fetch_manifestation`] refuses any URL outside this
/// prefix, so the egress the engine manifest declares is enforced by
/// code, not only by the grep-level conformance gate: a manifestation
/// URL the graph might one day point elsewhere becomes a typed
/// refusal, never a silent connection to an undeclared host.
pub const MANIFESTATION_HOST: &str = "https://fedlex.data.admin.ch/";

/// Polite, identifying user agent (endpoint etiquette).
pub const USER_AGENT: &str = "oh-mcp-fedlex/0.2 (openhelvetia.swiss; base-tier domain server)";

/// How long a single live request may take before it is given up
/// (rule J17.5: the endpoint publishes no rate limit, and an unbounded
/// request would hold a thread for ever if a connection hangs). Two
/// classes, chosen against the CALLER's patience rather than against
/// the endpoint's: the chat gives a tool call 15 s and the polite brake
/// may already have reserved 5 s of that, so a select that runs longer
/// is answering nobody — while a manifestation is a 400 KB file and
/// deserves twice as long.
pub const SELECT_TIMEOUT: Duration = Duration::from_secs(15);
pub const FETCH_TIMEOUT: Duration = Duration::from_secs(30);

/// The other bound on a manifestation: a body larger than this is not
/// read at all. Named here so the refusal can carry it (J17.5).
pub const MAX_BODY_MIB: u64 = 16;

/// The four live paths, and how each names ITSELF in a refusal: the
/// class (which request, and whether it failed on the call or while
/// reading the body) and the bound that applied. They are functions so
/// the suite can pin the wording without a network — a stalled
/// connection would prove ureq's behaviour, not this server's choice
/// (J17.5).
pub fn select_call_class() -> String {
    format!("SPARQL select (timeout {} s)", SELECT_TIMEOUT.as_secs())
}

/// The body half of a SELECT — the same bound, a different half.
pub fn select_body_class() -> String {
    format!(
        "SPARQL result body (select, timeout {} s)",
        SELECT_TIMEOUT.as_secs()
    )
}

/// The call half of a manifestation fetch.
pub fn fetch_call_class() -> String {
    format!(
        "manifestation fetch (timeout {} s)",
        FETCH_TIMEOUT.as_secs()
    )
}

/// The body half of a manifestation fetch — bounded in time AND size.
pub fn fetch_body_class() -> String {
    format!(
        "manifestation body (fetch, timeout {} s, at most {MAX_BODY_MIB} MiB)",
        FETCH_TIMEOUT.as_secs()
    )
}

/// How an answer's manifestation reached the server — the honesty
/// marker beside `transaction_time`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Served {
    /// Fetched from the federal host for this very call.
    Live,
    /// Served from the in-process cache; `retrieved_at` is the moment
    /// of the ORIGINAL fetch, not of this call.
    Cache,
    /// Read from a recorded fixture (the offline test and demo path).
    Fixture,
}

impl Served {
    pub fn as_str(self) -> &'static str {
        match self {
            Served::Live => "live",
            Served::Cache => "cache",
            Served::Fixture => "fixture",
        }
    }
}

/// A manifestation as the domain receives it: the body, how it was
/// served and — for live/cached bodies — when it was really fetched
/// (RFC 3339, UTC). `None` for fixtures, which carry no retrieval
/// moment of their own (the injected `today` stands in).
pub struct Fetched {
    pub body: Arc<str>,
    pub served: Served,
    pub retrieved_at: Option<String>,
}

/// One cached manifestation: the resolved URL, the body and the
/// moment it was fetched from the federal host.
#[derive(Debug, Clone)]
pub struct CachedManifestation {
    pub url: String,
    pub body: Arc<str>,
    pub retrieved_at: String,
}

impl CachedManifestation {
    fn bytes(&self) -> usize {
        self.url.len() + self.body.len()
    }
}

/// Default cap of the live cache: 64 MiB of manifestation bodies.
pub const DEFAULT_CACHE_BYTES: usize = 64 * 1024 * 1024;
/// Default cap of the live cache: entries (one per version × language).
pub const DEFAULT_CACHE_ENTRIES: usize = 256;

/// The bounded in-process LRU of fetched manifestations.
///
/// Keyed by the semantic manifestation key
/// (`manifestation:<version>:<lang>`) — the same key the fixture uses,
/// so a cache line and a recorded line name the same thing. Bounded
/// in bytes AND entries; a body larger than the byte cap is never
/// cached (served live every time, honestly). Least recently USED is
/// evicted first (a hit moves the entry to the back). No persistence:
/// the cache dies with the process.
pub struct ManifestationCache {
    inner: Mutex<CacheInner>,
    max_bytes: usize,
    max_entries: usize,
}

struct CacheInner {
    /// Front = least recently used.
    entries: VecDeque<(String, Arc<CachedManifestation>)>,
    bytes: usize,
}

impl ManifestationCache {
    pub fn new(max_bytes: usize, max_entries: usize) -> Self {
        Self {
            inner: Mutex::new(CacheInner {
                entries: VecDeque::new(),
                bytes: 0,
            }),
            max_bytes,
            max_entries,
        }
    }

    /// The default-sized cache (64 MiB, 256 entries).
    pub fn default_sized() -> Self {
        Self::new(DEFAULT_CACHE_BYTES, DEFAULT_CACHE_ENTRIES)
    }

    /// A hit: the entry, moved to the most-recently-used end.
    pub fn get(&self, key: &str) -> Option<Arc<CachedManifestation>> {
        let mut inner = self.inner.lock().expect("cache lock not poisoned");
        let pos = inner.entries.iter().position(|(k, _)| k == key)?;
        let entry = inner.entries.remove(pos).expect("position exists");
        let hit = entry.1.clone();
        inner.entries.push_back(entry);
        Some(hit)
    }

    /// Stores an entry, evicting least recently used ones until the
    /// caps hold. A body beyond the byte cap is not stored at all.
    pub fn put(&self, key: &str, value: CachedManifestation) {
        let size = value.bytes();
        if size > self.max_bytes || self.max_entries == 0 {
            return;
        }
        let mut inner = self.inner.lock().expect("cache lock not poisoned");
        if let Some(pos) = inner.entries.iter().position(|(k, _)| k == key) {
            let (_, old) = inner.entries.remove(pos).expect("position exists");
            inner.bytes -= old.bytes();
        }
        while !inner.entries.is_empty()
            && (inner.bytes + size > self.max_bytes || inner.entries.len() >= self.max_entries)
        {
            if let Some((_, evicted)) = inner.entries.pop_front() {
                inner.bytes -= evicted.bytes();
            }
        }
        inner.bytes += size;
        inner.entries.push_back((key.to_string(), Arc::new(value)));
    }

    pub fn len(&self) -> usize {
        self.inner
            .lock()
            .expect("cache lock not poisoned")
            .entries
            .len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn bytes(&self) -> usize {
        self.inner.lock().expect("cache lock not poisoned").bytes
    }
}

// ---------------------------------------------------------------------
// The polite brake and the semantic fixture store now live in
// `oh-mcp-common` (BX): this server built both (BS, BQ) and the LINDAS
// server needs the same mechanism against a different host. What stays
// HERE is the wording — a refusal names a host, and the host is this
// server's.
// ---------------------------------------------------------------------

pub use oh_mcp_common::fixtures::{fixture_file_name, index_line, key_file, now_rfc3339};
pub use oh_mcp_common::throttle::{
    busy_retry_after_ms, FrozenClock, UpstreamBusy, UpstreamThrottle, DEFAULT_UPSTREAM_BURST,
    DEFAULT_UPSTREAM_MAX_WAIT, DEFAULT_UPSTREAM_RATE,
};

/// The refusal as this backend raises it; [`busy_retry_after_ms`]
/// reads it back on every error path, hand-written or bridged.
pub fn busy_message(throttle: &UpstreamThrottle, busy: UpstreamBusy) -> String {
    let ms = busy.retry_after.as_millis();
    format!(
        "upstream-busy: retry_after_ms={ms}: the polite brake against \
         fedlex.data.admin.ch is saturated ({} live requests/s, burst {}); this \
         request would have waited longer than {} s — retry after {ms} ms",
        throttle.rate_per_second(),
        throttle.burst(),
        throttle.max_wait().as_secs_f64()
    )
}

pub enum Backend {
    /// Live queries against the public endpoint, manifestations
    /// cached in-process (bounded LRU, see [`ManifestationCache`]).
    Live {
        endpoint: String,
        cache: ManifestationCache,
        /// The polite brake over every live request (BS).
        throttle: UpstreamThrottle,
    },
    /// Recorded responses under `dir/<key-hash>.json` — the test
    /// path; also written by the recording run. Never cached: a
    /// fixture IS the recorded reality, reading it costs nobody
    /// anything.
    Fixtures { dir: PathBuf },
    /// Live, and every response is also written as a fixture
    /// (the deliberate re-record path, `--record`). Never cached: the
    /// point of a recording pass is to fetch. Braked like Live.
    Recording {
        endpoint: String,
        dir: PathBuf,
        throttle: UpstreamThrottle,
    },
    /// The test double of `Live`: answers from recorded files AS IF
    /// fetched — every select and every manifestation fetch is
    /// counted, every fetch is stamped with the next value of the
    /// injected clock, and the cache works exactly as in `Live`. This
    /// is how an offline test proves «two reads, one fetch» without a
    /// network.
    Counting {
        dir: PathBuf,
        cache: ManifestationCache,
        selects: Arc<AtomicUsize>,
        fetches: Arc<AtomicUsize>,
        /// Retrieval stamps handed out in order, one per fetch (RFC
        /// 3339); when exhausted the last one repeats.
        clock: Mutex<VecDeque<String>>,
        /// The brake under test, on a frozen clock — or none.
        throttle: Option<UpstreamThrottle>,
        /// Every SPARQL query this double was asked to run, in order —
        /// so a test can hold the queries themselves against a rule
        /// (BT′: the «from»-free guard over the whole consultation
        /// path, the vendored primitives' queries included).
        seen: Mutex<Vec<String>>,
        /// Every fixture KEY the double was asked for, in order — the
        /// keys are what a recording pass writes as files, and the
        /// difference between their number and the number of requests
        /// is what nested calls cost (BV A′ point 5).
        seen_keys: Mutex<Vec<String>>,
    },
}

fn live_select(endpoint: &str, query: &str) -> Result<serde_json::Value> {
    let mut response = ureq::post(endpoint)
        .config()
        .timeout_global(Some(SELECT_TIMEOUT))
        .build()
        .header("accept", "application/sparql-results+json")
        .header("user-agent", USER_AGENT)
        .send_form([("query", query)])
        .map_err(|e| match e {
            // The endpoint REJECTED the query (4xx): a permanent answer
            // about the input — the WAF or the parser refused what was
            // built from it — not a transient outage. Kept apart so the
            // domain can type it `invalid-input`.
            ureq::Error::StatusCode(code) if (400..500).contains(&code) => {
                anyhow::anyhow!("bad-request: HTTP {code}")
            }
            other => anyhow::anyhow!("upstream-unavailable: {}: {other}", select_call_class()),
        })?;
    response
        .body_mut()
        .read_json::<serde_json::Value>()
        // The BODY read is the second half of the same request: a
        // stall here is bounded by the same global timeout, and the
        // refusal names class and bound exactly like the call path
        // (J17.5 — a reader must not have to guess which limit bit).
        .map_err(|e| anyhow::anyhow!("upstream-unavailable: {}: {e}", select_body_class()))
}

fn read_fixture(dir: &std::path::Path, key: &str) -> Result<serde_json::Value> {
    let path = key_file(dir, key);
    let raw = std::fs::read_to_string(&path).with_context(|| {
        format!(
            "fixture missing for key «{key}» ({}) — run the recording pass \
             (cargo test --test e2e record_fixtures -- --ignored)",
            path.display()
        )
    })?;
    Ok(serde_json::from_str(&raw)?)
}

fn read_fixture_text(dir: &std::path::Path, key: &str) -> Result<String> {
    let path = key_file(dir, key).with_extension("xml");
    std::fs::read_to_string(&path)
        .with_context(|| format!("fixture missing for key «{key}» ({})", path.display()))
}

fn live_fetch(url: &str) -> Result<String> {
    let mut response = ureq::get(url)
        .config()
        .timeout_global(Some(FETCH_TIMEOUT))
        .build()
        .header("user-agent", USER_AGENT)
        .call()
        .map_err(|e| anyhow::anyhow!("upstream-unavailable: {}: {e}", fetch_call_class()))?;
    response
        .body_mut()
        .with_config()
        .limit(MAX_BODY_MIB * 1024 * 1024)
        .read_to_string()
        .map_err(|e| anyhow::anyhow!("upstream-unavailable: {}: {e}", fetch_body_class()))
}

impl Backend {
    /// The live backend with the default-sized manifestation cache and
    /// the default polite brake.
    pub fn live(endpoint: impl Into<String>) -> Self {
        Self::live_with_throttle(endpoint, UpstreamThrottle::default_polite())
    }

    /// The live backend with a brake of the caller's choosing
    /// (`--upstream-rate` at the binaries).
    pub fn live_with_throttle(endpoint: impl Into<String>, throttle: UpstreamThrottle) -> Self {
        Backend::Live {
            endpoint: endpoint.into(),
            cache: ManifestationCache::default_sized(),
            throttle,
        }
    }

    /// The recording backend (live, every answer written as a
    /// fixture), braked like Live.
    pub fn recording(endpoint: impl Into<String>, dir: impl Into<PathBuf>) -> Self {
        Backend::Recording {
            endpoint: endpoint.into(),
            dir: dir.into(),
            throttle: UpstreamThrottle::default_polite(),
        }
    }

    /// The counting test double over a fixture directory (see the
    /// variant's documentation).
    pub fn counting(
        dir: impl Into<PathBuf>,
        cache: ManifestationCache,
        clock: Vec<String>,
    ) -> (Self, Arc<AtomicUsize>, Arc<AtomicUsize>) {
        Self::counting_with(dir, cache, clock, None)
    }

    /// The counting test double WITH a brake (on a frozen clock, so a
    /// test measures waits instead of taking them).
    pub fn counting_throttled(
        dir: impl Into<PathBuf>,
        cache: ManifestationCache,
        clock: Vec<String>,
        throttle: UpstreamThrottle,
    ) -> (Self, Arc<AtomicUsize>, Arc<AtomicUsize>) {
        Self::counting_with(dir, cache, clock, Some(throttle))
    }

    fn counting_with(
        dir: impl Into<PathBuf>,
        cache: ManifestationCache,
        clock: Vec<String>,
        throttle: Option<UpstreamThrottle>,
    ) -> (Self, Arc<AtomicUsize>, Arc<AtomicUsize>) {
        let selects = Arc::new(AtomicUsize::new(0));
        let fetches = Arc::new(AtomicUsize::new(0));
        let backend = Backend::Counting {
            dir: dir.into(),
            cache,
            selects: selects.clone(),
            fetches: fetches.clone(),
            clock: Mutex::new(clock.into()),
            throttle,
            seen: Mutex::new(Vec::new()),
            seen_keys: Mutex::new(Vec::new()),
        };
        (backend, selects, fetches)
    }

    /// Every query the counting double was asked to run, in order.
    /// Empty for every other backend.
    pub fn seen_queries(&self) -> Vec<String> {
        match self {
            Backend::Counting { seen, .. } => seen.lock().expect("seen lock not poisoned").clone(),
            _ => Vec::new(),
        }
    }

    /// Every fixture key the counting double was asked for, in order.
    /// Empty for every other backend. A recording pass writes ONE file
    /// per distinct key while making one request per entry here.
    pub fn seen_keys(&self) -> Vec<String> {
        match self {
            Backend::Counting { seen_keys, .. } => {
                seen_keys.lock().expect("seen lock not poisoned").clone()
            }
            _ => Vec::new(),
        }
    }

    /// The brake this backend runs its live requests through — none
    /// for Fixtures and for an unbraked Counting double.
    pub fn throttle(&self) -> Option<&UpstreamThrottle> {
        match self {
            Backend::Live { throttle, .. } | Backend::Recording { throttle, .. } => Some(throttle),
            Backend::Counting { throttle, .. } => throttle.as_ref(),
            Backend::Fixtures { .. } => None,
        }
    }

    /// Takes a token from the brake before a live request — or raises
    /// the `upstream-busy` text the domain types.
    fn brake(&self) -> Result<()> {
        let Some(throttle) = self.throttle() else {
            return Ok(());
        };
        throttle
            .acquire()
            .map(|_| ())
            .map_err(|busy| anyhow::anyhow!("{}", busy_message(throttle, busy)))
    }

    /// Runs a SELECT/ASK. `key` is the stable semantic fixture key
    /// («tool:param=value»); the raw query is what actually runs.
    pub fn select(&self, key: &str, query: &str) -> Result<serde_json::Value> {
        match self {
            Backend::Live { endpoint, .. } => {
                self.brake()?;
                live_select(endpoint, query)
            }
            Backend::Fixtures { dir } => read_fixture(dir, key),
            Backend::Counting {
                dir,
                selects,
                seen,
                seen_keys,
                ..
            } => {
                self.brake()?;
                selects.fetch_add(1, Ordering::SeqCst);
                seen.lock()
                    .expect("seen lock not poisoned")
                    .push(query.to_string());
                seen_keys
                    .lock()
                    .expect("seen lock not poisoned")
                    .push(key.to_string());
                read_fixture(dir, key)
            }
            Backend::Recording { endpoint, dir, .. } => {
                self.brake()?;
                let value = live_select(endpoint, query)?;
                std::fs::create_dir_all(dir)?;
                let path = key_file(dir, key);
                let mut pretty = serde_json::to_string_pretty(&value)?;
                pretty.push('\n');
                std::fs::write(&path, pretty)?;
                // The key→file mapping stays human-auditable.
                index_line(dir, &path, key)?;
                Ok(value)
            }
        }
    }

    /// The cache line for a manifestation key, if this backend caches
    /// and holds one. Fixtures and Recording never do.
    pub fn cached_manifestation(&self, key: &str) -> Option<Arc<CachedManifestation>> {
        match self {
            Backend::Live { cache, .. } | Backend::Counting { cache, .. } => cache.get(key),
            Backend::Fixtures { .. } | Backend::Recording { .. } => None,
        }
    }

    /// Remembers a fetched manifestation under its semantic key —
    /// only backends that fetch for real keep a cache.
    pub fn remember_manifestation(&self, key: &str, url: &str, fetched: &Fetched) {
        let Some(retrieved_at) = &fetched.retrieved_at else {
            return;
        };
        match self {
            Backend::Live { cache, .. } | Backend::Counting { cache, .. } => cache.put(
                key,
                CachedManifestation {
                    url: url.to_string(),
                    body: fetched.body.clone(),
                    retrieved_at: retrieved_at.clone(),
                },
            ),
            Backend::Fixtures { .. } | Backend::Recording { .. } => {}
        }
    }

    /// Fetches a manifestation (Akoma-Ntoso XML) — Live fetches from
    /// the federal host and stamps the moment, Fixtures reads the
    /// recorded `<key-hash>.xml`, Recording fetches AND writes the
    /// fixture, Counting reads the fixture as if fetched. The URL must
    /// lie under [`MANIFESTATION_HOST`] — the declared egress, enforced
    /// before any connection.
    pub fn fetch_manifestation(&self, key: &str, url: &str) -> Result<Fetched> {
        if !url.starts_with(MANIFESTATION_HOST) {
            bail!(
                "upstream-unavailable: manifestation URL «{url}» is outside the declared \
                 egress host {MANIFESTATION_HOST} — refused, never fetched"
            );
        }
        match self {
            Backend::Live { .. } => {
                self.brake()?;
                let body = live_fetch(url)?;
                Ok(Fetched {
                    body: body.into(),
                    served: Served::Live,
                    retrieved_at: Some(now_rfc3339()),
                })
            }
            Backend::Fixtures { dir } => Ok(Fetched {
                body: read_fixture_text(dir, key)?.into(),
                served: Served::Fixture,
                retrieved_at: None,
            }),
            Backend::Counting {
                dir,
                fetches,
                clock,
                ..
            } => {
                self.brake()?;
                let body = read_fixture_text(dir, key)?;
                fetches.fetch_add(1, Ordering::SeqCst);
                let stamp = {
                    let mut clock = clock.lock().expect("clock lock not poisoned");
                    if clock.len() > 1 {
                        clock.pop_front()
                    } else {
                        clock.front().cloned()
                    }
                };
                Ok(Fetched {
                    body: body.into(),
                    served: Served::Live,
                    retrieved_at: Some(stamp.unwrap_or_else(|| "1970-01-01T00:00:00Z".into())),
                })
            }
            Backend::Recording { dir, .. } => {
                self.brake()?;
                let body = live_fetch(url)?;
                std::fs::create_dir_all(dir)?;
                let path = key_file(dir, key).with_extension("xml");
                std::fs::write(&path, &body)?;
                index_line(dir, &path, key)?;
                Ok(Fetched {
                    body: body.into(),
                    served: Served::Live,
                    retrieved_at: Some(now_rfc3339()),
                })
            }
        }
    }

    /// The bindings array of a SELECT result.
    pub fn bindings(value: &serde_json::Value) -> Result<&Vec<serde_json::Value>> {
        value
            .pointer("/results/bindings")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| anyhow::anyhow!("upstream-unavailable: malformed SPARQL JSON"))
    }
}

/// The bridge from the vendored `fedlex-jolux` primitives onto this
/// backend.
///
/// One `KeyedClient` per tool call: the first query a primitive
/// issues runs under the call's semantic key, every further one
/// under `<key>:q2`, `<key>:q3` … (`get_article_history` asks twice —
/// upstream split its query to stay under the federal WAF's ~600-
/// character «SELECT … from» trap, see fedlex-jolux `impacts.rs`).
/// The keys are deterministic per call, so the recording pass and the
/// offline run see the same files.
pub struct KeyedClient<'a> {
    backend: &'a Backend,
    key: String,
    issued: AtomicU32,
}

impl<'a> KeyedClient<'a> {
    pub fn new(backend: &'a Backend, key: impl Into<String>) -> Self {
        Self {
            backend,
            key: key.into(),
            issued: AtomicU32::new(0),
        }
    }

    fn query_sync(
        &self,
        sparql: &str,
    ) -> Result<fedlex_jolux::SparqlResults, fedlex_jolux::JoluxError> {
        let n = self.issued.fetch_add(1, Ordering::SeqCst) + 1;
        let key = if n == 1 {
            self.key.clone()
        } else {
            format!("{}:q{n}", self.key)
        };
        let value = self.backend.select(&key, sparql).map_err(|e| {
            let text = format!("{e:#}");
            match text
                .strip_prefix("bad-request: HTTP ")
                .and_then(|code| code.trim().parse::<u16>().ok())
            {
                Some(code) => fedlex_jolux::JoluxError::BadRequest(code),
                None => fedlex_jolux::JoluxError::Transport(text),
            }
        })?;
        serde_json::from_value(value)
            .map_err(|e| fedlex_jolux::JoluxError::MalformedResults(e.to_string()))
    }
}

impl fedlex_jolux::SparqlClient for KeyedClient<'_> {
    // The vendored trait is `#[async_trait]`; this is its expanded
    // signature, implemented without the macro so the crate adds no
    // direct dependency. The answer is computed synchronously and
    // handed back as an already-completed future.
    fn query<'life0, 'life1, 'async_trait>(
        &'life0 self,
        sparql: &'life1 str,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = Result<fedlex_jolux::SparqlResults, fedlex_jolux::JoluxError>,
                > + Send
                + 'async_trait,
        >,
    >
    where
        'life0: 'async_trait,
        'life1: 'async_trait,
        Self: 'async_trait,
    {
        let result = self.query_sync(sparql);
        Box::pin(async move { result })
    }
}

/// Drives a vendored async primitive to completion WITHOUT a runtime.
///
/// Every await inside those primitives is a [`KeyedClient::query`],
/// which is ready on its first poll — so a single poll with a no-op
/// waker completes the future. `None` means the future suspended on
/// something else, which no vendored primitive does; the caller turns
/// that into an honest upstream refusal rather than spinning.
pub fn drive<F: std::future::Future>(future: F) -> Option<F::Output> {
    let mut future = std::pin::pin!(future);
    let waker = std::task::Waker::noop();
    let mut cx = std::task::Context::from_waker(waker);
    match future.as_mut().poll(&mut cx) {
        std::task::Poll::Ready(value) => Some(value),
        std::task::Poll::Pending => None,
    }
}

/// SPARQL string-literal escaping for interpolated user input —
/// injection is refused by construction.
pub fn sparql_escape(input: &str) -> Result<String> {
    if input.chars().any(|c| c.is_control()) {
        bail!("invalid-input: control characters");
    }
    Ok(input.replace('\\', "\\\\").replace('"', "\\\""))
}

/// IRI safety for interpolation into `<…>`: refuse anything that
/// could break out of the IRI position.
pub fn iri_safe(input: &str) -> Result<&str> {
    if input.is_empty()
        || !input.starts_with("https://fedlex.data.admin.ch/eli/")
        || input
            .chars()
            .any(|c| c.is_whitespace() || c == '<' || c == '>' || c == '"')
    {
        bail!("invalid-input: not a Fedlex ELI IRI");
    }
    Ok(input)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(url: &str, body: &str) -> CachedManifestation {
        CachedManifestation {
            url: url.into(),
            body: body.into(),
            retrieved_at: "2026-08-29T10:00:00Z".into(),
        }
    }

    /// Least recently USED goes first: a hit moves an entry to the
    /// back, so a later insertion evicts the untouched one.
    #[test]
    fn cache_evicts_least_recently_used_by_entries() {
        let cache = ManifestationCache::new(1 << 20, 2);
        cache.put("a", entry("u", "aaaa"));
        cache.put("b", entry("u", "bbbb"));
        assert!(cache.get("a").is_some(), "a touched → most recent");
        cache.put("c", entry("u", "cccc"));
        assert!(cache.get("b").is_none(), "b was the least recently used");
        assert!(cache.get("a").is_some());
        assert!(cache.get("c").is_some());
        assert_eq!(cache.len(), 2);
    }

    /// The byte cap evicts as many old entries as the new one needs;
    /// a body beyond the cap is never stored.
    #[test]
    fn cache_honours_the_byte_cap() {
        let cache = ManifestationCache::new(20, 100);
        cache.put("a", entry("u", "12345678")); // 9 bytes
        cache.put("b", entry("u", "12345678")); // 18
        assert_eq!(cache.bytes(), 18);
        cache.put("c", entry("u", "1234")); // needs 5 → evicts a
        assert!(cache.get("a").is_none());
        assert_eq!(cache.bytes(), 14);
        cache.put("huge", entry("u", &"x".repeat(40)));
        assert!(cache.get("huge").is_none(), "beyond the cap: not stored");
        assert_eq!(cache.len(), 2);
    }

    fn frozen_brake(
        rate: f64,
        burst: f64,
        max_wait_s: u64,
    ) -> (UpstreamThrottle, Arc<FrozenClock>) {
        let clock = FrozenClock::new();
        let brake =
            UpstreamThrottle::frozen(rate, burst, Duration::from_secs(max_wait_s), clock.clone());
        (brake, clock)
    }

    /// Six requests within one frozen second at 2/s, burst 4: four
    /// take a burst token, the fifth and sixth reserve and wait 500
    /// and 1000 ms — recorded, not slept.
    #[test]
    fn the_brake_admits_the_burst_and_queues_the_rest() {
        let (brake, clock) = frozen_brake(2.0, 4.0, 5);
        let waits: Vec<u128> = (0..6)
            .map(|_| brake.acquire().expect("admitted").as_millis())
            .collect();
        assert_eq!(waits, vec![0, 0, 0, 0, 500, 1000]);
        assert_eq!(
            clock
                .sleeps()
                .iter()
                .map(Duration::as_millis)
                .collect::<Vec<_>>(),
            vec![500, 1000]
        );
        assert_eq!(brake.admitted(), 6);
        assert_eq!(brake.refused(), 0);
    }

    /// Twenty requests at once: the burst, then ten reservations up to
    /// the five-second limit (fourteen admitted, in order), then six
    /// refusals that reserve nothing and all name the same retry —
    /// 5500 ms, the wait a free token is away.
    #[test]
    fn beyond_the_wait_limit_the_brake_refuses_without_reserving() {
        let (brake, clock) = frozen_brake(2.0, 4.0, 5);
        let outcomes: Vec<Result<u128, u128>> = (0..20)
            .map(|_| {
                brake
                    .acquire()
                    .map(|w| w.as_millis())
                    .map_err(|b| b.retry_after.as_millis())
            })
            .collect();
        let admitted: Vec<u128> = outcomes.iter().filter_map(|o| o.ok()).collect();
        let refused: Vec<u128> = outcomes.iter().filter_map(|o| o.err()).collect();
        assert_eq!(admitted.len(), 14);
        assert_eq!(
            admitted,
            vec![0, 0, 0, 0, 500, 1000, 1500, 2000, 2500, 3000, 3500, 4000, 4500, 5000]
        );
        assert_eq!(refused, vec![5500; 6]);
        assert!(
            outcomes[..14].iter().all(Result::is_ok),
            "admissions come first, in arrival order"
        );
        assert_eq!(brake.admitted(), 14);
        assert_eq!(brake.refused(), 6);
        // Time passes on the frozen clock: two seconds refill four
        // tokens against the ten reserved — a deficit of seven, a wait
        // of 3.5 s, admitted again.
        clock.advance(Duration::from_secs(2));
        assert_eq!(brake.acquire().expect("admitted").as_millis(), 3500);
        // And a long pause refills to the burst, never beyond it.
        clock.advance(Duration::from_secs(600));
        let waits: Vec<u128> = (0..5)
            .map(|_| brake.acquire().expect("admitted").as_millis())
            .collect();
        assert_eq!(waits, vec![0, 0, 0, 0, 500]);
    }

    /// The refusal text round-trips its retry_after_ms through the
    /// parser the domain uses on both error paths.
    #[test]
    fn the_busy_message_carries_a_parseable_retry() {
        let (brake, _) = frozen_brake(2.0, 4.0, 5);
        let text = busy_message(
            &brake,
            UpstreamBusy {
                retry_after: Duration::from_millis(5500),
            },
        );
        assert_eq!(busy_retry_after_ms(&text), Some(5500));
        assert!(text.starts_with("upstream-busy: retry_after_ms=5500: "));
        assert_eq!(busy_retry_after_ms("upstream-unavailable: timeout"), None);
        assert_eq!(
            busy_retry_after_ms("SPARQL transport error: upstream-busy: retry_after_ms=750: x"),
            Some(750)
        );
    }

    /// The live requests carry an identifying agent and a generous
    /// timeout — the rulebook's J17.5 asks for at least thirty
    /// seconds, and an unbounded request is what BV came to fix.
    #[test]
    fn the_timeout_constants_bound_every_live_request_against_the_caller() {
        // What this test proves is the CONSTANTS — the agent string
        // and the two bounds every live path passes to ureq. It does
        // not stall a connection: the timeout's effect is ureq's, the
        // choice of the bound is ours, and that choice is what is
        // pinned here (BV A′: the name says what the test does).
        assert!(USER_AGENT.contains("openhelvetia.swiss"));
        assert!(USER_AGENT.starts_with("oh-mcp-fedlex/"));
        // Bounded against the CALLER's patience: the chat's tool budget
        // is 15 s and the brake may reserve 5 s of it, so a select that
        // outlives it is answering nobody; a manifestation is a file and
        // gets twice as long. Neither may be unbounded.
        assert_eq!(SELECT_TIMEOUT, Duration::from_secs(15));
        assert_eq!(FETCH_TIMEOUT, Duration::from_secs(30));
        assert!(SELECT_TIMEOUT >= DEFAULT_UPSTREAM_MAX_WAIT + Duration::from_secs(5));
        assert!(FETCH_TIMEOUT > SELECT_TIMEOUT);
        // …and each of the FOUR live paths names its class and its
        // bound in the refusal — call and body, select and fetch. This
        // is the wording J17.5 claims; without it the row is untested.
        assert_eq!(select_call_class(), "SPARQL select (timeout 15 s)");
        assert_eq!(
            select_body_class(),
            "SPARQL result body (select, timeout 15 s)"
        );
        assert_eq!(fetch_call_class(), "manifestation fetch (timeout 30 s)");
        assert_eq!(
            fetch_body_class(),
            "manifestation body (fetch, timeout 30 s, at most 16 MiB)"
        );
        for class in [
            select_call_class(),
            select_body_class(),
            fetch_call_class(),
            fetch_body_class(),
        ] {
            assert!(class.contains("timeout"), "{class}");
            assert!(
                class.contains("15 s") || class.contains("30 s"),
                "the bound is IN the refusal: {class}"
            );
        }
        // The body halves say so; the call halves do not claim to be them.
        assert!(select_body_class().contains("body") && !select_call_class().contains("body"));
        assert!(fetch_body_class().contains("body") && !fetch_call_class().contains("body"));
    }

    /// Re-storing a key replaces it and re-counts its bytes.
    #[test]
    fn cache_replaces_an_existing_key() {
        let cache = ManifestationCache::new(1 << 20, 10);
        cache.put("a", entry("u", "short"));
        cache.put("a", entry("u", "a much longer body"));
        assert_eq!(cache.len(), 1);
        assert_eq!(cache.bytes(), 1 + "a much longer body".len());
        assert_eq!(&*cache.get("a").unwrap().body, "a much longer body");
    }
}
