# fedlex domain server — v0 base-tier tool contract (PROPOSAL)

<!-- language: English -->

**Proposal for the platform's OWN server (L2.2), derived from
REFERENCE.md — spec only, the audit session builds the core.** Names
follow the platform's capability-id style (dot notation
`<domäne>.<verb_objekt>`, the gateway convention: tool names ARE
capability ids). Every deviation from the prototype is marked
**«deviation + grounds»** so the rebuild can state what it changed
and why. v0 is the smallest honest spine: eight tools that make the
bitemporal citation loop work end to end; the remaining reference
pools (treaties, consultations, AS/BBl, vocabulary, tables) are v1+
candidates, listed at the end — never silently dropped.

## Cross-cutting contract (all tools)

- **Provenance mandatory** (adopted from the reference): every
  content answer carries `eli`, `valid_as_of`, `transaction_time`;
  discovery answers carry `kind: "hint"`, content answers
  `kind: "norm"`. No provenance, no norm content.
- **`as_of` param everywhere** (adopted): optional ISO date; absent
  means «today», echoed back resolved — never implicit.
- **Honest not-found:** an unknown SR number, ELI or eId returns a
  typed error `{error: "not-found", subject: <echoed input>}` — never
  an empty success, never a guess. Malformed input returns
  `{error: "invalid-input", detail}`. Upstream SPARQL failure returns
  `{error: "upstream-unavailable"}` — the server never fabricates
  from cache what the query could not prove (cache serving is marked
  in `transaction_time` semantics, not hidden).
- **Deviation + grounds (auth):** the prototype is fail-closed with
  in-server JWT roles. The platform server exposes the base tier
  UNAUTHENTICATED BEHIND THE GATEWAY: policy (auth via `can()`, rate
  limits, budget) lives in the L2.3 gateway per E11/E16 — one policy
  layer platform-wide instead of per-server re-implementation. The
  domain server stays pure domain logic; fail-closed moves to the
  gateway boundary.
- **Deviation + grounds (names):** dot-notation capability ids
  (`fedlex.read_article` instead of `read_article`) — the sealed
  gateway convention; the verb_objekt part stays deliberately close
  to the reference so operators can map surfaces 1:1.
- **Deviation + grounds (probes):** the server's manifest declares
  the L0.8 probe hint (`mcp-initialize`/`response`) — cold-start
  agents never guess a safe call; the reference's corpus example
  predates the probe extension.
- **Deviation + grounds (quota weight):** the reference's
  cache-vs-live cost distinction is kept but enforced in the gateway
  budget layer (E11-WP7), not in the server.

## The eight v0 tools

1. **`fedlex.resolve_sr`** — SR number → ELI.
   Params: `{sr: string}`. Response: `{eli, title (lang map), status,
   kind: "norm"}`. Errors: not-found (unknown SR). SPARQL pattern:
   lookup over `jolux:` taxonomy/`jolux:classifiedByTaxonomyEntry`
   resp. the SR notation property on the consolidation abstract.
2. **`fedlex.search_law`** — title/keyword → candidates.
   Params: `{query: string, limit?: int}`. Response: `{hits: [{eli,
   title, score?}], kind: "hint"}` — hints, made binding only by a
   subsequent norm proof (reference semantics kept). Errors:
   invalid-input on empty query. SPARQL: text filter over titles
   (`jolux:title`/`dcterms` labels) on `jolux:ConsolidationAbstract`.
3. **`fedlex.get_law_metadata`** — ELI → JOLux profile.
   Params: `{eli, as_of?}`. Response: `{title, abbreviation?, sr?,
   status, dates{…}, kind: "norm", provenance}`. Errors: not-found.
   SPARQL: CBD of the consolidation abstract (the ELI graph pattern
   verified in S3: named graph `<eli>/graph`).
4. **`fedlex.list_versions`** — consolidations of an act.
   Params: `{eli}`. Response: `{versions: [{eli_version, date,
   in_force_from?}], kind: "norm"}`. SPARQL:
   `jolux:isMemberOf`/consolidation relations of the abstract.
5. **`fedlex.resolve_consolidation_at`** — the governing version at a
   date (the bitemporal core). Params: `{eli, as_of}`. Response:
   `{eli_version, valid_as_of, kind: "norm"}`. Errors: not-found
   (no version in force at that date — honest, e.g. before first
   entry into force). SPARQL: date-filtered consolidation selection
   (applicability intervals on `jolux:Consolidation`).
6. **`fedlex.read_article`** — eId-precise text of a version.
   Params: `{eli_version, eid, lang?}`. Response: `{eid, text,
   structure_refs?, kind: "norm", provenance incl. valid_as_of}`.
   Errors: not-found (unknown eId in that version — the S3 probe
   showed eId-level resources exist in the official graph, e.g.
   `…/art_10a/1`). Source: manifestation fetch (XML/Akoma-Ntoso) of
   the expression, eId-addressed; cached, cache-marked.
7. **`fedlex.get_citations`** — citation relations of an act/article.
   Params: `{eli, eid?, direction: "in"|"out"}`. Response:
   `{citations: [{from, to, kind_of_ref?}], kind: "norm"}`. SPARQL:
   `jolux:` citation/impact relations (impacts split kept:
   in/out mirrors the reference's `get_impacts`/
   `get_outgoing_impacts` collapsed into one tool with a direction
   param — **deviation + grounds:** one tool with a typed direction
   is one capability id and halves the surface without losing
   anything; the response names the direction explicitly).
8. **`fedlex.check_in_force`** — in force at date? Params:
   `{eli, as_of}`. Response: `{in_force: bool, governing_version?,
   kind: "norm"}` — false is a VALID answer, not an error (honest
   negatives are content here). SPARQL: applicability interval check.

## Explicitly v1+ (never silently dropped)

**Status at BQ (2026-08-29): twelve of these are BUILT** —
`get_structure`, `search_text`, `read_document`, `get_references`,
`get_modifications` (with the change notes), `list_annexes`,
`get_article_history`, `get_subdivisions`, `get_taxonomy`,
`list_expressions`, `resolve_vocabulary_label`, `find_related_topic`;
`read_element` is `fedlex.read_article` (any eId-bearing element,
path eIds included). The full upstream → `fedlex.<id>` → status map
is TOOLSET-v1.md; the list below stays as the v0 record.

Treaties (`find_treaties`, `get_treaty_info`), consultations
(`get_consultations`, `get_consultation_documents`), AS/BBl
(`get_oc_act`, `get_memorial`, `get_fga_documents`, `get_drafts`),
vocabularies (`resolve_vocabulary_label`, `list_vocabulary`),
structure extras (`get_structure`, `list_components`,
`extract_tables`, `detect_foreign_content`, `extract_change_notes`,
`parse_unlinked_ref`, `get_subdivisions`, `list_annexes`,
`list_expressions`, `get_article_history`, `get_taxonomy`,
`get_modifications`, `search_text`, `read_element`, `read_document`,
`explore_node`, `find_related_topic`), and the composite
`compare_versions`. The semantic tier (embedding search) is its own
server per E15's family cut; the generative tier is not a domain
server concern.

## Open inputs

- Governance — **corrected 21.08.2026 against the SEALED E15 wording
  (audit session, on Jonathan's challenge):** E15 nowhere forbids
  code reuse; it makes the fedlex family the FIRST INSTANCE of the
  engine pattern and normalizes contract + strategy, not binary
  form. The public repo's **Apache-2.0 license IS the published
  license act** — reuse is permitted with attribution (LICENSE/
  NOTICE retention) and the house provenance discipline (source,
  commit, hash, retrieval date — the A2A-schema pattern). What
  remains as hygiene, not blocker: a board transparency note
  (authors are board-adjacent, Art.-12 disclosure culture). The
  earlier «needs Alex's explicit license act» phrasing was the
  assignment's own over-caution, not a corpus rule.
- ~~The manifestation-cache strategy for `fedlex.read_article`~~ —
  **CLOSED at BO′ (29.08.2026):** a bounded in-process LRU in the live
  backend (TOOLSET-v1.md, «The manifestation cache»), marked in the
  provenance exactly as this contract demanded — `served: cache` and
  the original retrieval moment as `transaction_time`. Earlier state,
  kept for the record: **v0 resolved 21.08.2026 without a cache:** on-demand fetch of the
  governing XML manifestation (vendored fedlex-jolux FRBR pattern) +
  vendored fedlex-akn extraction; fixtures freeze the recorded
  reality for tests. A mirror/cache layer stays an E11 cost-lens
  follow-up for volume, not a correctness need.
- Conformance testing pattern: adopt the reference's
  lexicon-projection idea (offline test keeps capability register
  and tool surface congruent) using the platform's own capability
  register — fits the E16 gate-5 discipline.
