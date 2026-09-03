//! `oh-mcp-fedlex` — the fedlex domain MCP server, base tier
//! (masterplan L2.2, home `mcp/servers` per sealed E06; M2 of the
//! sealed E04 work plan). Built AGAINST the reference-behavior spec
//! (REFERENCE.md / TOOLSET-v0.md, assignment S): the platform's OWN
//! server, oriented on the running prototype's proven behavior — and
//! since the vendored-AKN binding ALSO standing on its published
//! Apache-2.0 code (third_party/mcp-fedlex, PROVENANCE.md).
//! Corrected 21.08.2026 (Reflexionsschleife lens 2): E15 normalizes
//! contract + strategy and draws no code boundary — the earlier
//! «never its code» line predated the sealed-E15 re-read and
//! contradicted this crate's own dependency graph.
//!
//! Base tier: stateless queries over the PUBLIC Fedlex SPARQL
//! endpoint of the federal administration — normal use, polite
//! agent, no campaigns. Policy (auth, rate, budget) lives at the
//! L2.3 gateway boundary per the TOOLSET deviation notes; this
//! server is pure domain logic.

pub mod backend;
pub mod domain;
pub mod server;
