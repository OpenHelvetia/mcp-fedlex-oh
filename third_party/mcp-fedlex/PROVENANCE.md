# Provenance: vendored mcp-fedlex crates

<!-- language: English -->

- **What:** three library crates vendored from the upstream fedlex
  reference workspace — `fedlex-akn` (Akoma-Ntoso/eId text access,
  chunking, structure), `fedlex-jolux` (JOLux SPARQL primitives:
  temporal resolution, citations, search, vocabularies) and
  `fedlex-core` (shared types: ELI/eId newtypes, bitemporal
  provenance, response envelopes). The assignment ordered the first
  two; **`fedlex-core` is vendored out of technical necessity** —
  both ordered crates depend on it by path
  (`fedlex-core = { path = "../fedlex-core" }`) and it is not
  published on crates.io (verified 2026-08-21: crates.io API answers
  «crate `fedlex-core` does not exist»); without it nothing builds.
- **Source URL:** https://github.com/mindful-bio/mcp-fedlex
- **Commit pin:** `64e0ec3fdce3cc6841fa9f659456a520c1e21083`
  (upstream commit date 2026-07-09).
- **Retrieved:** 2026-08-21T08:15:30Z (git clone, checkout of the
  pinned commit; crate trees copied unchanged — byte identity against
  the clone proven with `diff -r` per crate).
- **Per-crate SHA-256 over the vendored tree** (command:
  `find <crate> -type f -print0 | sort -z | xargs -0 shasum -a 256 |
  shasum -a 256`, run in this directory):
  - `fedlex-core`
    `7a4e7b79a0ff59783a3fc339428469ff54b2c987ddf6f8c8feadccf9cf1240fb`
  - `fedlex-akn`
    `19d30862c54f42e146c2b218dc6211c934e67e2d5444496ede1f825a4634e59a`
  - `fedlex-jolux`
    `68288f4e66605d7d858ccc8e54f7dbaf8bbbee92983478c4b7bb9bdc7e02aca5`
  - `LICENSE`
    `dc43dff79b28acf8c8017847534816d23931d0003d3eb33ab239c248b63eed00`

## License

Upstream is published under **Apache-2.0** (workspace-wide
`license = "Apache-2.0"`, authors `mindful.bio`); the upstream
`LICENSE` file is retained here **verbatim** (hash above). The
upstream repository carries **no NOTICE file** (checked at the pinned
commit: `find . -iname "NOTICE*"` — empty), so there is none to
retain. Attribution obligation is met by this file + the retained
LICENSE; the crates' own `Cargo.toml` files keep their upstream
license/authors/repository fields untouched (resolved via the
workspace stub, below).

## Board transparency note

The upstream authors are **board-adjacent** (mindful.bio GmbH — Alex
Camenzind; the fedlex family is the platform's editorial reference
project no. 1). Per the Art.-12 disclosure culture this fact is
**disclosed to the board as a note — it is not a consent
requirement**: the sealed-E15 re-read (commit `f2cfe33`) established
that the published Apache-2.0 license IS the license act; reuse takes
attribution + this provenance discipline, nothing more.

## Local changes (exhaustive)

The vendored crate trees are byte-identical to upstream. Everything
else that exists in this directory is an OpenHelvetia addition:

1. **`Cargo.toml` (workspace stub, new file):** the crates'
   manifests use `*.workspace = true` fields and `[lints]
   workspace = true`, which need a workspace to resolve. The stub
   replicates the upstream `[workspace.package]`,
   `[workspace.lints.clippy]` and `[profile.release]` values
   **verbatim**; only the member list is shortened to the vendored
   subset. Zero edits inside the crates' own manifests — the sibling
   layout keeps their relative `path = "../fedlex-core"` dependencies
   valid as-is.
2. **`Cargo.lock`:** copied from the upstream workspace (preserves
   upstream-tested dependency versions), then (a) pruned by the first
   in-tree build (cargo removes the packages only the non-vendored
   members used) and (b) **one security update**:
   `cargo update -p quinn-proto` 0.11.14 → 0.11.17 for
   **RUSTSEC-2026-0185** (remote memory exhaustion, high, fix
   ≥0.11.15; dev-dependency chain via reqwest — fixable in our lock,
   so fixed rather than accepted; the update also dropped
   `zerocopy 0.8.50` from the tree). `cargo audit` clean afterwards
   (RC=0, 145 dependencies, 2026-08-21).
3. **Not carried over:** upstream `rust-toolchain.toml` (pins only
   `channel = "stable"` + rustfmt/clippy — the repo toolchain rule
   already covers this), the remaining workspace members
   (fedlex-store, fedlex-bridge, fedlex-telemetry, mcp-reader — not
   ordered, not needed by the two target crates), and all
   docs/scripts/docker files.

## Verification (fresh figures, 2026-08-21)

- `cargo test` in this directory: **144 passed, 0 failed**
  (fedlex-akn 46 · fedlex-core 29 · fedlex-jolux 69), **41 live
  tests ignored by upstream design** (`#[ignore = "E2E gegen
  Live-Fedlex"]` — the default run is fully offline; live runs are a
  deliberate act against the public endpoint, upstream politeness
  rules apply: `--test-threads 2`).
- `cargo-audit =0.22.2` over the lock: clean (after the quinn-proto
  update). `cargo-deny =0.20.2` with the root `deny.toml`:
  advisories/licenses/sources ok (the rkyv/h2 acceptance warnings
  are «advisory-not-detected» — those acceptances belong to other
  trees and simply do not apply here).

## Update rule

New upstream state = **new vendoring commit** (fresh clone at a new
pinned commit, fresh `diff -r` proof, fresh hashes, this file
updated) — never an in-place edit of the vendored trees. Register
rows in `docs/reference/working-rules.md` §5 carry the pin.

## Boundary

These crates are vendored as **integration material for
oh-mcp-fedlex** (`mcp/servers/fedlex/`) — binding `read_article`
onto `fedlex-akn` and deepening the temporal/citation loop with
`fedlex-jolux` is the build session's cut, not part of the vendoring
commit. Until that integration lands, nothing in the platform links
against these crates; they are material, not a served claim (E15:
reference behavior documented in `mcp/servers/fedlex/REFERENCE.md`;
reuse per the corrected governance note in TOOLSET-v0.md).
