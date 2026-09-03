# oh-mcp-fedlex

An MCP server by the association [OpenHelvetia](https://openhelvetia.swiss) over the Confederation's [Fedlex](https://www.fedlex.admin.ch) infrastructure: Swiss federal law through the public SPARQL endpoint and the official Akoma Ntoso texts. 35 tools — resolve an SR number to an ELI, list the versions of an act, determine the consolidation in force at a date, read an article eId-precise, check a quote against the norm text that was read, write the canonical citation. Every answer carries its source. Stateless, no account.

The data stays with the Confederation. This repository is the interface.

## Run it

```bash
cargo run --locked --manifest-path mcp/servers/fedlex/Cargo.toml -- --help
```

## Test it

Every test answers from recorded fixtures (102 files under semantic keys); nothing reaches the network. The live recording runs are marked `#[ignore]`.

```bash
cargo test --locked --manifest-path mcp/servers/fedlex/Cargo.toml
```

## What is in here

| Path | What |
|---|---|
| `mcp/servers/fedlex/` | the server: `TOOLSET-v1.md` (the contract), `ENGINE.md`, `REFERENCE.md`, `engine.manifest.json`, sources, tests, fixtures |
| `mcp/servers/common/` | what the platform's domain MCP servers share: the polite brake and the semantic fixture store |
| `third_party/mcp-fedlex/` | three library crates vendored from the upstream fedlex reference workspace, Apache-2.0, provenance in `PROVENANCE.md` |
| `docs/reference/fedlex-data-rules.md` | the rulebook (123 rules) the conformance table is gated against |

## Where it comes from

Published by the association's publication lane from its corpus at commit `45dad0c` (2026-09-03). The module's card with state, evidence and dependencies: <https://openhelvetia.swiss/en/directory/building-blocks/fedlex-engine/>. Its guide: <https://openhelvetia.swiss/en/docs/infrastructure/module-fedlex-engine/>.

Issues here are welcome; changes go through the corpus and arrive with the next publication. Security reports, in confidence: security@openhelvetia.swiss.

## Licence

Apache-2.0 (see `LICENSE`, attribution in `NOTICE`); the vendored upstream crates under `third_party/` carry their own `LICENSE`.
