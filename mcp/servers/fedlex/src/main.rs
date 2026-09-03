//! Entry point: the fedlex domain server over stdio.
//!
//! Usage:
//!   oh-mcp-fedlex [--fixtures <dir>] [--endpoint <url>]
//!                 [--upstream-rate <n/s>] [--upstream-burst <n>]
//!
//! Default: live against the public Fedlex SPARQL endpoint (polite
//! agent, single queries per tool call). `--fixtures` runs entirely
//! offline on recorded responses (the test path — also the honest
//! demo path without network). The streamable-HTTP deployment rides
//! the gateway/ld.* switch-on.
//!
//! `--upstream-rate` / `--upstream-burst` (BS): the polite brake over
//! every live request to the federal host — 2 requests per second
//! sustained and a burst of 4 by default; a request that would wait
//! longer than five seconds answers the typed `upstream-busy` with
//! `retry_after_ms`. Cache hits and fixtures are never braked.

use std::sync::Arc;

use rmcp::{transport::stdio, ServiceExt};

use oh_mcp_fedlex::backend::{
    Backend, UpstreamThrottle, DEFAULT_UPSTREAM_BURST, DEFAULT_UPSTREAM_MAX_WAIT,
    DEFAULT_UPSTREAM_RATE, FEDLEX_ENDPOINT,
};
use oh_mcp_fedlex::domain::Ctx;
use oh_mcp_fedlex::server::FedlexServer;

fn today() -> String {
    // Live entry point stamps the system date (the LIBRARY never
    // reads a clock — injection point per the house discipline).
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock before 1970")
        .as_secs() as i64;
    let days = now.div_euclid(86400);
    // civil_from_days (Hinnant) — small and exact.
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

#[tokio::main]
async fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut take = |flag: &str| -> Option<String> {
        let pos = args.iter().position(|a| a == flag)?;
        if pos + 1 >= args.len() {
            return None;
        }
        let v = args.remove(pos + 1);
        args.remove(pos);
        Some(v)
    };
    let rate: f64 = take("--upstream-rate")
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_UPSTREAM_RATE);
    let burst: f64 = take("--upstream-burst")
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_UPSTREAM_BURST);
    let backend = match take("--fixtures") {
        Some(dir) => Backend::Fixtures { dir: dir.into() },
        None => Backend::live_with_throttle(
            take("--endpoint").unwrap_or_else(|| FEDLEX_ENDPOINT.to_string()),
            UpstreamThrottle::new(rate, burst, DEFAULT_UPSTREAM_MAX_WAIT),
        ),
    };
    let server = FedlexServer {
        ctx: Arc::new(Ctx {
            backend,
            today: today(),
        }),
    };
    let running = match server.serve(stdio()).await {
        Ok(running) => running,
        Err(error) => {
            eprintln!("error: MCP serve failed: {error}");
            std::process::exit(1);
        }
    };
    if let Err(error) = running.waiting().await {
        eprintln!("error: MCP session ended abnormally: {error}");
        std::process::exit(1);
    }
}
