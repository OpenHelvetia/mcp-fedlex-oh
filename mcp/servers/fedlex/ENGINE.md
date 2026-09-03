# fedlex engine — E15 contract + strategy description

<!-- language: English -->

**The E15 clause-1 artifacts for the fedlex engine:** the strategy
description (this file) and the engine manifest
(`engine.manifest.json`, egress-conformance-tested). E15 normalizes
for every engine «den Kontrakt (Eingabe-Typen des Bestands, erzeugtes
Manifest inkl. Stufen-Deklaration, capabilities) und die
Strategie-Beschreibung (Chunking, Graphmodell, Retrieval, Eval —
lernbar und auf eigene Bestände übertragbar), keine Binärform»
(docs/decisions/E15, Ziff. 1). **Pre-standard status:** the
`engines/standard` 0.x specification is an open E15 work package
(Konsequenz 2); until it exists, this pair is the first-instance
extraction E15 itself calls for («Muster-Extraktion aus der
fedlex-Familie als erste Katalog-Dokumentation») — shape implied by
the sealed wording, to be migrated onto the standard schema when the
commission ratifies one.

Everything below is **harvested, never invented**: sources are the
vendored crates (`third_party/mcp-fedlex/`, byte-identical at commit
`64e0ec3…`, see its PROVENANCE.md), the upstream `docs/` tree at the
same commit (cited as `upstream:docs/…` — not vendored), and the
platform's own REFERENCE.md (assignment S, retrieval-dated).

## 1. The tier ensemble (Stufen-Deklaration)

Per REFERENCE.md §catalogue: **all four tool pools of the reference
reader are base tier** — stateless navigation over the official data,
cache + live SPARQL, no index of its own. The **semantic** tier is a
sister component (reference: `mcp-fedlex-semantic`, embedding search
with its own indexer); the **skills** tier composes primitives into
auditable reasoning steps (reference: `mcp-fedlex-skills`); the
generative layer above (ansV) is a *usage* context, not a domain
server concern. This is exactly E15's «gestuftes Ensemble —
Basis-Zugriff (günstig, zustandslos), Semantik-Stufe samt Indexer, wo
sie sich lohnt, Skills obendrauf».

Platform status per tier is declared honestly in
`engine.manifest.json`: **base is built** (oh-mcp-fedlex, this
directory: thirty-five tools — the eight-tool v0 spine, the
twelve-tool navigator surface of BQ wave 1, the thirteen tools of BR
wave 2 and the citation pair of BT, every one proven offline on
recorded fixtures; TOOLSET-v1.md maps the whole upstream catalogue
onto them — every row v0, v1 or never with its reason — and names the
two ids beyond it: `check_quote` proves a quote's wording where the
norm text lies, `cite` writes the canonical Fundstelle of an eId); **semantic and generative
are not built** — declared as planned tiers with their reference
shape, never as served capability.

**Two-stage discovery is part of the contract (E16 Ziff. 1).** Stage
one is the one-line inventory — `tools/list` here, `meta.tools` at
the gateway — where every tool's line is ≤ 160 characters, begins
with the verb, says WHEN to use it and whether it answers a `hint` or
a `norm`, and carries the trigger words a question would contain (SR,
Artikel, Absatz, Anhang, Fassung, Verweis, Änderung). Stage two loads
the input schemas of the three to five tools a model intends to call
(`meta.schemas`). The server's `instructions` name both stages and
the loop: find the act → pick the version (and see whether it is
PDF-only) → find the place WITHOUT guessing article numbers
(`get_structure`, `search_text`) → read it → cite only a norm.

## 2. Strategy: chunking

Source: `third_party/mcp-fedlex/fedlex-akn/src/chunking.rs` (module
docs; upstream rulebook ids X14/X20).

- **Hollowing before chunking (AKN-CHK-01):** in Akoma-Ntoso,
  parent-element text is the concatenation of its children — measured
  **87.1 % redundancy** (X20.2); naive per-eId indexing puts every
  sentence 3–4× into the index. Hollowing keeps text only on **eId
  leaves**, parents become placeholders naming their children — for
  the Energiegesetz that is 117'647 → ~15'156 characters (X20.1).
- **Pattern-dependent chunk cut (AKN-CHK-02):** the document is
  first classified, then cut per pattern — STRUCTURED/FLAT_ARTICLES
  per article (oversized ones per paragraph), LEVEL_BASED per level
  leaf, AMENDMENT per `<mod>`, NO_BODY per non-stub component, OTHER
  as `<p>` groups. One chunking rule for all shapes would be wrong
  for most Swiss acts.
- **Two text views, strictly separated:** the citable norm text
  (`text.rs`) is never the retrieval text — chunks carry an
  **enriched retrieval view** (markdown tables, `[Historie: …]`
  notes, resolved ref links, `[Formel]`/`[Grafik]` placeholders). A
  guard test (`chunk_enrichment_never_leaks_into_reader_text`)
  enforces that enrichment can never leak into what an agent would
  cite. *Transferable lesson:* retrieval representation and quotable
  source are different artifacts; conflating them corrupts citations.

## 3. Strategy: graph model

Source: `upstream:docs/dev/10_LEXICON_jolux.md` + ADR-010 (both at
commit `64e0ec3…`).

- The corpus metadata lives in the official **JOLux ontology** (19
  classes, 65 predicates, 46 SKOS vocabularies), consumed as-is via
  the public SPARQL endpoint — the engine builds **no graph of its
  own**; derivations stay reproducible from the source (E15
  Leitsatz). JOLux carries metadata, **never law text** (upstream
  rule J0.1) — text comes from Akoma-Ntoso manifestations.
- The ontology is filtered through **empirical reality**: fill rates
  and behavior verified over **69'350 consolidation entries**,
  captured as a rulebook (J0–J20) — the graph model is the ontology
  *as it is actually populated*, not as documented.
- The complete function space over this model is enumerated as a
  **capability lexicon (47 primitives)** — «kein Tool-Katalog eines
  bestimmten Servers, sondern das Vokabular, aus dem Konsumenten
  komponieren». Tools are **projections** of the lexicon (ADR-010
  projected the full space onto 40 reader tools), and a conformance
  test keeps projection and lexicon congruent. *Transferable
  lesson:* enumerate the holding's function space independently of
  any server, then project — the lexicon survives tool redesigns.

## 4. Strategy: retrieval

Source: `upstream:docs/handbuch/HANDBUCH.en.md` §3–§5, ADR-003,
ADR-011; REFERENCE.md (provenance model).

- **Finding the act (BO′):** `search_law` resolves official
  abbreviations exactly over `jolux:titleShort` before it searches
  titles and popular names, and ranks in-force acts first and then by
  the systematic collection's own order — the law before its
  ordinances — so the act that governs a field stands first instead
  of whichever ordinance the graph returned first.
- **Two structurally distinct answer kinds:** every response is a
  **norm citation** (`kind: "norm"` — quotable state of a named act
  at a resolved date) or a **discovery hint** (`kind: "hint"` —
  candidate, never quotable). The correct loop is «find a discovery
  hint → read it with a norm-citation tool → only then cite»; the
  format split makes recording a search hit as substantiated norm
  **structurally impossible**.
- **Bitemporal point-in-time:** `as_of` is a first-class tool
  argument everywhere (ADR-011); the server resolves and **stamps
  server-side** what version actually governed (`valid_as_of` +
  `transaction_time`) — the client may request a date, only the
  server writes what applied.
- **Fixed processing chain, fail-closed:** credential → RBAC → quota
  → execute → provenance stamp, no shortcuts; in doubt the server
  refuses (upstream handbook §3). The platform port keeps the chain
  but moves policy (auth/rate/budget) to the L2.3 gateway boundary —
  TOOLSET-v0.md, deviation + grounds. What stays IN the server is a
  courtesy, not a policy: the **polite brake** (BS) — one token
  bucket over every live request to the federal host, SPARQL selects
  and manifestation fetches alike, 2 a second sustained, burst 4, at
  most 5 s of waiting; beyond that the request is refused at once as
  `upstream-busy` with `retry_after_ms`. Cache hits and fixtures are
  never braked. «Single polite requests, no campaigns» is thereby a
  property of the code.
- **Typed refusals — the contract's error side.** A refusal is an
  answer with a body, never prose to parse; machines branch on
  `error`. Four kinds are the server's, two the gateway's:

  | `error` | decided by | body | means | recovers |
  |---|---|---|---|---|
  | `not-found` | the server, after the query | `subject` (echoed input), `detail` where the graph knows why | the graph carries no such thing; **false is an answer too** (`check_in_force`) | no |
  | `invalid-input` | the server, before any query — or after the endpoint's 4xx (a query built from the input that the WAF or parser rejected) | `detail` | the question was malformed; a retry with the same value fails again | no |
  | `upstream-unavailable` | the server | `detail` | the endpoint did not answer, or answered malformed; **never fabricated from the cache** — a cached manifestation is served only as what it is | the source's state |
  | `upstream-busy` (BS) | the server's polite brake | `detail`, `retry_after_ms` | the brake is saturated and this request would have waited longer than the limit; the endpoint is fine, WE decline to hammer it; the gateway refunds the call's weight, since nothing reached the endpoint | **yes**, after `retry_after_ms` |
  | `budget-exhausted` | the gateway (E11) | `detail` | the session's weighted budget is spent | a new session |
  | `rate-limited` (BS) | the gateway | `detail`, `retry_after_ms` | the client's per-interval bucket is empty; the refused call cost no budget | **yes**, after `retry_after_ms` — HTTP 429 with `Retry-After` on the JSON door |

  The gateway's README carries the same table with the door mapping
  (status codes, tool errors).
- **Two sources, two families, one contract (BQ):** the JOLux tools
  run one or two SPARQL queries per call — the BQ ones THROUGH the
  vendored `fedlex-jolux` primitives over a synchronous bridge
  (`backend::KeyedClient`: one semantic fixture key per call, `:q2`
  for a primitive's follow-up query), so upstream's WAF-safe query
  shapes and rulebook knowledge come along. The XML tools resolve the
  version's Akoma-Ntoso manifestation — one query, one fetch, then
  (since BO′) a bounded in-process cache line per version and
  language, so a research loop reading five articles of one act
  fetches its 433 KB once; a cached answer says `served: cache` and
  keeps the real retrieval moment as `transaction_time` — and answer
  from the vendored `fedlex-akn` layer: outline, substring
  search over the eId leaves, capped whole document, references,
  amendment notes, annexes. Where a vendored primitive answers a
  narrower question than the tool — or, as the review found for the
  article history, a wrong one (substring match on the eId) — the
  server asks its own query in the primitive's shape and says so.
  Path eIds (`art_2/para_1`,
  `annex_u1/lvl_u1`) travel unchanged from the tools that find a
  place to the tool that reads it.
- **Caps are answers, not cuts:** every list carries `truncated` and
  its original size; where the graph cannot count, one row beyond
  the cap is requested so the flag is measured.
- **Ingestion side (semantic tier, reference):** release events
  materialize the corpus into three sinks (DOM/reference store,
  JOLux graph store, vector index); an **embedding outbox** plus DLQ
  guards against *silent index drift* (ADR-003) — consistency between
  corpus and index is a tested property, not an assumption.

## 5. Strategy: eval

Source: `third_party/mcp-fedlex/fedlex-{akn,jolux}/tests/
lexicon_conformance.rs`; `upstream:docs/dev/10_LEXICON_jolux.md`
(Methodik).

- **Lexicon conformance suites:** one live test **per lexicon
  primitive** (41 across the two vendored crates), each checking two
  levels — *capability* (the Rust primitive delivers live data
  end-to-end) and *expectation* (the empirical rulebook claims —
  fill rates, direction semantics, fall traps — still hold). Audit
  tests additionally pin the **explicitly excluded phantom
  predicates** (documented-but-empty ontology parts stay excluded).
- **Offline by default, polite by design:** every live test is
  `#[ignore]`; live runs are a deliberate act with
  `--test-threads 2` against the public endpoint. The default
  `cargo test` proves the offline logic (144 tests in the vendored
  tree).
- *Transferable lesson:* eval is not a benchmark bolted on at the
  end — it is the rulebook (empirical claims about the holding) made
  executable, re-checkable against the living source.

## 6. Manifest, capabilities, egress

`engine.manifest.json` (this directory) declares: the **input types**
of the holding (JOLux RDF via SPARQL; Akoma-Ntoso XML
manifestations), the **tier declaration** with resource/cost profile
per E15 Ziff. 2, the **generated-manifest pointer** (marked NOT YET
SERVED — the platform manifest goes live with the ld.* switch-on;
described-but-dead resources are forbidden), and the **capabilities
with the exhaustive egress list** per E15 Ziff. 3a (syntax
`Domain[:Port]`):

> Egress: `fedlex.data.admin.ch:443` — and nothing else. Re-verified
> at BQ against the recorded manifestations: the graph's
> `jolux:isExemplifiedBy` URLs (22 across XML/PDF/HTML/DOCX, five
> languages, three acts) all live under
> `fedlex.data.admin.ch/filestore/…`, not under the human portal
> `www.fedlex.admin.ch` the assignment expected — so no second host
> is declared, and the server ENFORCES the one host at runtime
> (`backend::MANIFESTATION_HOST`: a manifestation URL outside the
> prefix is refused, never fetched).

**Conformance test (method stated):** the gate is
`engines/standard/conformance/`, wired into `tools/check.sh`. It moved
there when the engine standard was written (L4.5): a gate that checks
engines against the standard belongs with the standard, and its
vocabulary-IRI allowlist — which used to live in the gate's own code —
is now declared per engine in `capabilities.iri_namespace_hosts`,
because an allowlist inside a shared gate would let one engine's
reviewed exception silently cover another engine's host. The method is
**grep-level over the server sources** (E15 3a's runtime enforcement
— network-policy artifacts on k3s — is the deploy step's cut): it
extracts every `https://<host>` literal from `src/*.rs` of
oh-mcp-fedlex and fails unless the host set **equals** the declared
egress host set — a host in code but not declared breaks the gate,
and a declared host no code reaches breaks it too (no
described-but-dead egress). Limits stated honestly: literal-level
extraction cannot see dynamically composed URLs; the server
constructs none today (the endpoint is a single `const`), and the
gate keeps it that way — a dynamic endpoint would have to be declared
as a literal to pass.


## 7. Component map — where the prototype's parts land in the platform

*(Recorded 21.08.2026 from Jonathan's architecture sorting, verified
against the sealed corpus — the decisions already place each part.)*

| Prototype component | Platform home | Grounds |
|---|---|---|
| `mcp-fedlex` (navigate) | **this engine's base tier** (mcp/servers/fedlex — built: 20 of its 40 tools, TOOLSET-v1.md maps the rest) | E15 names the trio «navigieren · suchen · schliessen»; base = stateless/cheap |
| `mcp-fedlex-semantic` (+ indexer) | **the semantic tier of THIS ensemble** — a sister server, phase 2 | E15 Ziff. 1/2 (tier with own resource/cost profile), E11 (paid tiers begin where platform resources are spent) — «premium» is literally the sealed model |
| `mcp-fedlex-skills` | **content in the GENERAL skill mechanism** (skills/ per VISION §4, L4.4; format per E08, listing per the SUITES.md mechanics) | the format is platform-general, the skill is domain content — that resolves the «fedlex-specific yet ecosystem-integrated» tension; E15: skills are the thin closing layer, generative outputs stay marked drafts |
| `mcp-fedlex-memory` | **NOT part of this engine** — the platform memory domain (memory/ per VISION §4, L4.1–L4.3; Portable Agent Memory is a standard candidate) | the sealed E15 trio deliberately EXCLUDES memory, and E15 rejects «engine fills memory/org» (E07 poisoning rules; «wir generieren keine Daten») — memory serves every infrastructure, not one domain |
| `ansV` (mounts all three) | the orchestrator pattern (L4.6 discovery & orchestration); its citizen face is the chat demonstrator (L5.2/M3) | VISION §5 two-stage discovery |

License nuance on record: `mcp-fedlex` is PUBLISHED (Apache-2.0 —
vendored with provenance, third_party/). The sister repos
(`-semantic`, `-skills`, `-memory`) are, as of this note, not known
to be published — reusing THEIR code needs mindful.bio's explicit
license/publication act first (the corrected E15 reading applies
only to published code). Named input for the phase-2/3 items.

## Citations

- E15 sealed wording: `docs/decisions/E15-engine-format-governance.md`
  (Ziff. 1–3; Konsequenz 2).
- Vendored sources (byte-identical, commit `64e0ec3…`, retrieved
  2026-08-21): `third_party/mcp-fedlex/` + its PROVENANCE.md.
- Upstream docs at the same commit (not vendored):
  `docs/handbuch/HANDBUCH.en.md`, `docs/dev/10_LEXICON_jolux.md`,
  `docs/dev/11_LEXICON_akn.md`, `docs/dev/adr/ADR-003`, `ADR-010`,
  `ADR-011` — repository github.com/mindful-bio/mcp-fedlex.
- Platform: `REFERENCE.md` (assignment S, retrieval-dated),
  `TOOLSET-v0.md` (v0 contract + deviations), `TOOLSET-v1.md` (the
  BQ navigator surface, upstream → `fedlex.<id>` → status map),
  `README.md` (build state).
