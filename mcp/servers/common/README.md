# oh-mcp-common — what two domain servers share

<!-- language: English -->

Two pieces, both extracted from the fedlex server at BX when the LINDAS
server needed them: **the polite brake** and **the semantic fixture
store**. Neither knows anything about SPARQL, a cube or an act — which
is exactly why they are here and not in either domain.

- **`throttle`** — one token bucket for every live request to an
  upstream host: a rate, a burst, and a maximum wait. A request that
  finds no token RESERVES the next one and waits for it, up to the
  bound; beyond that it is refused at once as `UpstreamBusy` carrying
  the moment a retry would find a token. `FrozenClock` makes all of
  that provable without sleeping: the suites step time instead of
  spending it.
- **`fixtures`** — the semantic fixture key: `key_file`,
  `fixture_file_name` (SHA-256, first eight bytes, hex + `.json`),
  `index_line` (the `<file> <key> <recorded>` line that keeps a fixture
  directory human-auditable) and `now_rfc3339` for the recording date.

## Who uses it

- [`../fedlex`](../fedlex) — `oh-mcp-fedlex`, the Fedlex domain server
  (35 tools). It built both pieces; see the attribution below.
- [`../lindas`](../lindas) — `oh-mcp-lindas`, the LINDAS `cube.link`
  domain server (8 tools).

Both mount behind [`../../gateway`](../../gateway), which is where
policy lives (auth, rate, budget — E11/E16); the brake in here is a
courtesy to a shared public endpoint, not a policy.

## Attribution

**This code is the fedlex server's, moved.** It was written there
(BS for the brake, the fixture store earlier), proven there, and given
away at BX rather than copied into the second server — the platform's
rule that one sentence has one home. What did NOT move is the WORDING
of the refusal: each server keeps its own `busy_message`, so a fedlex
refusal names `fedlex.data.admin.ch` and a LINDAS refusal names
`lindas.admin.ch`. A shared mechanism, two voices; sharing the sentence
would have made one server's error message a lie about the other's
endpoint.

## The pin test

`fixture_file_name("resolve_sr:candidates:832.10")` must still be
`6e01faeb23575d73.json`. That one assertion is what makes the move
provable rather than plausible: the fedlex fixture directory is
byte-identical across the extraction, so if the hashing rule had
shifted by a single byte, every recorded answer in it would have become
unreachable and this test would say so before any suite ran. The crate
carries **two** tests, both in `fixtures.rs`: that one
(`a_key_names_its_file_and_nothing_else_does`) and
`the_index_replaces_a_key_and_keeps_the_notes`, which pins that
re-recording a key REPLACES its line in `INDEX.txt` and leaves the
notes around it alone.

**The brake has no test of its own here, and that is worth saying out
loud:** its behaviour is pinned in the servers that use it — the fedlex
suite steps a `FrozenClock` through a saturated bucket (six calls in a
second, twenty at once) and the gateway's policy suite drives the same
clock. `FrozenClock` lives here so those suites can do that without
sleeping. A test of the bucket in this crate is owed; until it exists,
this paragraph says where the proof actually is rather than implying
one here.

Run: `cargo test --manifest-path mcp/servers/common/Cargo.toml`.
`tools/check.sh` holds this crate to `cargo fmt --check` beside the two
servers and the gateway.
