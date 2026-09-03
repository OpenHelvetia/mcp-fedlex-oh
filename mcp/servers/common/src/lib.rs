//! `oh-mcp-common` — what this platform's domain MCP servers share.
//!
//! Two servers now speak to two public federal SPARQL endpoints
//! (`mcp/servers/fedlex`, `mcp/servers/lindas`). What they share is not
//! domain logic — the family cut of E15 keeps that apart — but the two
//! mechanisms every polite reader of a public endpoint needs: a token
//! bucket that makes «single polite requests, no campaigns» a property
//! of the code, and a fixture store whose keys are semantic so a
//! recorded answer can be found, dated and re-recorded by hand.
//!
//! Extracted at BX from the fedlex server, which built both (BS, BQ).
//! The extraction changed no behaviour: the file names, the index
//! format and the bucket's reservation semantics are the same, and the
//! fedlex fixtures were not touched. What deliberately did NOT move is
//! the wording of a refusal — it names a host and belongs to the server
//! that has one — and the `Backend` enum itself, which in the fedlex
//! server carries a manifestation cache and an XML fetch path the
//! LINDAS server has no counterpart for.

pub mod fixtures;
pub mod throttle;
