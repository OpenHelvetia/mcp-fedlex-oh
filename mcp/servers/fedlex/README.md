# oh-mcp-fedlex — fedlex domain server, base tier (L2.2)

<!-- language: English -->

The platform's OWN fedlex server, built against the reference-behavior
spec (REFERENCE.md; TOOLSET-v0.md for the spine, TOOLSET-v1.md for
the navigator surface — assignment S, extended at BQ): the bitemporal
citation loop, in-act navigation and the citation pair as thirty-five MCP tools over the
PUBLIC Fedlex SPARQL endpoint of the federal administration and the
Akoma-Ntoso manifestations it points at. The upstream reference workspace
(Apache-2.0; source, commit and hashes in
`third_party/mcp-fedlex/PROVENANCE.md`) is integration
material with attribution + provenance discipline (E15 as sealed;
third_party/mcp-fedlex, PROVENANCE.md): the XML tools bind the
vendored `fedlex-akn` layer, the graph tools the vendored
`fedlex-jolux` primitives over a synchronous bridge
(`backend::KeyedClient`).

- **The v0 spine (8):** resolve_sr with in-force disambiguation and
  VISIBLE predecessors, search_law as hints (BO′: official
  abbreviations resolve exactly — StPO, OR, ZGB — and the act in
  force ranks first, the law before its ordinances), get_law_metadata,
  list_versions incl. future consolidations, resolve_consolidation_at,
  check_in_force with false-as-valid-answer and `future_as_of` for a
  projected Stichtag, get_citations over the
  verified foreseen-impact graph, read_article (eId-precise norm text,
  path eIds included).
- **The navigator surface (12, BQ wave 1, server version 0.2.0):** get_structure (the
  outline that ends guessing article numbers), search_text (in-act
  substring search, hits name their article), read_document (capped
  Markdown), get_references, get_modifications (amendment notes per
  element), list_annexes (path eIds, directly readable) — from the
  manifestation; get_article_history, get_subdivisions, get_taxonomy,
  list_expressions (PDF-only visible BEFORE a read),
  resolve_vocabulary_label, find_related_topic — from the graph.
  Every answer is `kind: norm` or `hint` with provenance; every list
  is capped with `truncated` and its original size.
- **Wave 2 (13, BR):** extract_tables (annex limit values as header
  and rows), parse_reference (a quoted Fundstelle — «Art. 7 Abs. 1
  lit. b LSV», «Anhang 3 Ziff. 2 LSV», «Art. 8 EMRK i.V.m. Art. 36 BV»
  — taken apart into the act, resolved by abbreviation, and an eId to
  read), get_citations directions cites|cited_by (the formal citation
  graph), compare_versions (added, removed, changed paragraphs with
  wording between two Fassungen), explore_node (a debugging view),
  detect_foreign_content; beyond the SR: find_treaties,
  get_treaty_info, get_consultations, get_consultation_documents,
  get_oc_act, get_memorial, get_fga_documents, get_drafts. Every
  upstream tool is now v0, v1 or never-with-reason (TOOLSET-v1.md).
- **Two-stage discovery (E16):** the description of every tool is
  the stage-one line — ≤ 160 characters, verb-first, says when to
  use it and whether it answers a hint or a norm; the gateway's
  `meta.tools` carries the same lines verbatim (test-pinned).
- **Offline by default in tests:** recorded fixtures with SEMANTIC
  keys (tests/fixtures/ + INDEX.txt): the BGÖ manifestation serves
  all seven XML tools without a request; JOLux keys, the KVG and the
  LSV manifestations were recorded at BQ. Re-record deliberately via
  `cargo test --test e2e record_fixtures -- --ignored --test-threads 1`
  (everything, sequential) — or one named set: `record_fixtures_bq`,
  `record_fixtures_taxonomy`, `record_fixtures_review` — always ONE
  test name per run, so no two recording passes ever run side by side
  against the endpoint; live smoke separately ignored. The proof the surface exists for is a test:
  `search_text «Zugang» → read_article` reaches Art. 17 BGÖ without
  anyone typing «17».
- **Egress:** one host, `fedlex.data.admin.ch` — endpoint AND
  manifestation files (recorded reality); enforced by
  `backend::MANIFESTATION_HOST`, declared in engine.manifest.json,
  gated by engines/standard/conformance. **Manifestation cache (BO′):**
  a bounded in-process LRU (64 MiB, 256 entries) in the live backend
  — five reads of one act fetch it once; a cached answer says
  `provenance.served: cache` and keeps the real retrieval moment as
  its `transaction_time`; fixtures say `fixture`. Weight stays 2.
- **The polite brake (BS):** every live request to the federal host —
  SPARQL selects and manifestation fetches alike — takes a token from
  one bucket: 2 a second sustained, burst 4 (`--upstream-rate <n/s>`,
  `--upstream-burst <n>`); a request without a token waits for the
  next one, up to 5 s, beyond that it is refused at once as the typed
  `upstream-busy` with `retry_after_ms` — the fourth error kind beside
  `not-found`, `invalid-input` and `upstream-unavailable` (ENGINE.md
  §4 has the table). Cache hits and fixtures are never braked. Proven
  offline on a frozen clock: 6 calls in a second → 4 at once, 2 after
  500 and 1000 ms; 20 at once → 14 in order, 6 refused with 5500 ms.
- Run: `oh-mcp-fedlex` (live, polite UA) or `--fixtures <dir>`
  (fully offline). Policy (auth/rate/budget) lives at the L2.3
  gateway boundary; the streamable-HTTP deployment rides the ld.*
  switch-on. Recording sets since BR: `record_fixtures_br`,
  `record_fixtures_br_eng`, `record_fixtures_bs` (the consultation
  keys).
