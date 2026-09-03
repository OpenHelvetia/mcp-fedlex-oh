# fedlex domain server — v1 tool contract (BQ: the navigator surface)

<!-- language: English -->

**Built.** v1 = the v0 spine (TOOLSET-v0.md, unchanged in its
semantics) plus the twelve navigator tools of BQ, the thirteen of BR
and the citation pair of BT — thirty-five, under the cross-cutting
contract of v0; three rules were sharpened at BQ and hold for every
tool:

- **Every list is capped, never cut silently:** a capped answer
  carries `truncated: true` and the original size (`total`,
  `nodes_total`, `total_chars`, …); where the graph cannot count for
  us, one row beyond the cap is asked for so `truncated` is a
  measurement, not a guess (own queries and the vendored list
  primitives that take a limit); where a vendored primitive caps
  inside itself (`get_subdivisions`, LIMIT 500) the answer names the
  cap and the basis instead.
- **Two stages of discovery (E16):** the stage-one line of every tool
  (its `description`, at the gateway `meta.tools`) is ≤ 160
  characters, begins with the verb, says WHEN to use the tool and ends
  with whether the answer is a `hint` or a `norm`; the e2e suite
  checks the rule, the gateway test pins the line verbatim.
- **Path eIds are first-class:** `art_2/para_1`, `annex_u1/lvl_u1` —
  what `get_structure`, `search_text` and `list_annexes` hand out,
  `read_article` reads (v0 accepted flat eIds only; found at BQ, the
  BO piece the repository did not carry).

Parameter forms as before: ELI = the full IRI
`https://fedlex.data.admin.ch/eli/…`, `eli_version` =
`<abstract>/<YYYYMMDD>`, `lang` de|fr|it|en|rm — for manifestations
too since BV A′ (J13.1: which languages a version carries as XML is
the graph's answer, not a rule of this server; `list_expressions`
shows it before a read, and a version without XML in the language
asked is a not-found with that ground);
`as_of` ISO stays a v0-spine parameter — the twelve BQ tools take a
version (their date IS the version's) or ask the graph as of today. Typed errors: `not-found` (echoes its
subject), `invalid-input` (before any query), `upstream-unavailable`
(never fabricated from cache: a cached manifestation is served ONLY
as what it is — `provenance.served: cache` with the moment of its
real retrieval — never in place of an answer the graph refused), and
since BS a fourth, `upstream-busy` `{error, detail, retry_after_ms}`:
the polite brake against the federal endpoint (2 requests/s, burst 4,
at most 5 s of waiting) is saturated and this request would have
waited longer than the limit — the endpoint is fine, the server
declines to hammer it; a retry after `retry_after_ms` finds a token.
The three older kinds are unchanged (ENGINE.md §4 has the table).

## The map: upstream name → `fedlex.<id>` → status

Reference: the 40-tool catalogue of the upstream reader
(REFERENCE.md; handbook `docs/handbuch/HANDBUCH.en.md` §7 at the
vendored pin). Status: **v0** (built at S/T) · **BQ** (built in this
wave) · **BR** (wave 2, planned — never silently dropped) · **never**
(with the reason).

| Upstream | `fedlex.<id>` | Status | Source / note |
|---|---|---|---|
| `resolve_sr_number` | `fedlex.resolve_sr` | v0 | own SPARQL; in-force disambiguation, predecessors visible |
| `search_law` | `fedlex.search_law` | v0, BO′ | own SPARQL; BO′: exact abbreviation pre-query on `jolux:titleShort` (StPO, OR, ZGB), titles + popular names, ranked in force first and by the systematic collection's order; `hint` on empty |
| `get_law_metadata` | `fedlex.get_law_metadata` | v0 | own SPARQL |
| `list_versions` | `fedlex.list_versions` | v0 | own SPARQL, future consolidations kept |
| `resolve_consolidation_at` | `fedlex.resolve_consolidation_at` | v0 | own SPARQL |
| `check_in_force` | `fedlex.check_in_force` | v0, BO′ | over `resolve_consolidation_at`; false is an answer; BO′: `future_as_of` marks a Stichtag after today as a projection |
| `get_impacts` + `get_outgoing_impacts` | `fedlex.get_citations` | v0 | ONE tool, `direction` in\|out (deviation + grounds in v0); foreseen-impact graph |
| `get_citations` (JLX-CIT-01, the formal citation relations) | `fedlex.get_citations` direction `cites` \| `cited_by` | **v1 (BR)** | fedlex-jolux `get_citations` over the bridge — two more DIRECTIONS of the one capability id (the v0 deviation, kept): formal citations at act level; **norm** |
| `read_article` + `read_element` | `fedlex.read_article` | v0 (path eIds: BQ) | fedlex-akn `get_element_text`; one tool reads any eId-bearing element, article or not |
| `get_structure` | `fedlex.get_structure` | **BQ** | fedlex-akn `get_document_structure`; `depth` article\|full; cap 3000 nodes; **norm** |
| `search_text` | `fedlex.search_text` | **BQ** | fedlex-akn `search_text` over eId leaves + the article/heading of every hit; `total`/`truncated`; **hint** |
| `read_document` | `fedlex.read_document` | **BQ** | fedlex-akn `get_readable_document`; `max_chars` (120 000 default, 400 000 max), `offset`, `total_chars`; **norm** |
| `get_references` | `fedlex.get_references` | **BQ** | fedlex-akn `get_all_references`, optional `eid` scope, paged; **hint** |
| `get_modifications` + `extract_change_notes` | `fedlex.get_modifications` | **BQ** (both folded) | fedlex-akn `extract_change_notes` (the per-element amendment notes of a consolidation — what the assignment asked for) AND `get_modifications` (the `<mod>` blocks of amending acts); one tool, two fields; **norm** |
| `list_components` (+ the JOLux `list_annexes`) | `fedlex.list_annexes` | **BQ** | fedlex-akn `list_components` + `get_component_document` + outline: titles, own work IRI, stub flag, PATH eIds; the JOLux annex view is reachable through `fedlex.get_subdivisions`; **norm** |
| `get_article_history` | `fedlex.get_article_history` | **BQ** | two own queries in the vendored JLX-IMP-02 shape but with an EXACT target (the vendored primitive matches the eId as a substring — `art_2` collected `art_20`…`art_23a`; found at the BQ review, an upstream finding), joined to the consolidation each impact opens; completeness caveat IN the answer; **norm** |
| `get_subdivisions` | `fedlex.get_subdivisions` | **BQ** | fedlex-jolux `get_subdivisions` (transitive — the `walk` field says so since A′, J17.3); gap catalogue, said so; **norm** |
| `get_taxonomy` | `fedlex.get_taxonomy` | **BQ** | own SPARQL (the vendored primitive answers one language and no notation): entries with notation, labels in every vocabulary language, and the `skos:broader` chain up to the SR branch; **norm** |
| `list_expressions` | `fedlex.list_expressions` | **BQ** | own SPARQL on the VERSION node (the vendored primitive lists languages across all consolidations and no formats): languages × manifestations (xml/pdf/html/docx, URLs), `xml_available`, `pdf_only`, `no_manifestation_listed`; **norm** |
| `resolve_vocabulary_label` + `list_vocabulary` | `fedlex.resolve_vocabulary_label` | **BQ** | fedlex-jolux `list_vocabulary` (label → IRIs inside a scheme) and, since BV A′, an own query for the IRI direction: every `skos:prefLabel` of the concept in one request, the fallback de → en → fr → it → rm choosing, `answered_in` / `label_lang` naming the language that answered and `labels_filled` counting the search matches that carried no label in the language asked (J5.4 — all twelve «Code» matches of `legal-subject-theme-fr` carry none); `language` answered from the vendored language table; **hint** |
| `find_related_topic` | `fedlex.find_related_topic` | **BQ** | fedlex-jolux `find_related_by_topic`; entry by ELI or SR; cap+1 truncation; **hint** |
| `get_metadata` (AKN FRBR self-description) | — | never | its content is served by `get_law_metadata` (JOLux profile) and `list_expressions` (languages/formats); the FRBR block is what `read_article` already resolves; a third profile tool would be a second wording |
| `find_treaties` | `fedlex.find_treaties` | **v1 (BR)** | own SPARQL in the vendored JLX-TRT-02 shape plus a title filter (the primitive filters country/bilateral only); cap+1; **hint** |
| `get_treaty_info` | `fedlex.get_treaty_info` | **v1 (BR)** | fedlex-jolux `get_treaty_info` over the bridge; **norm** |
| `get_consultations` | `fedlex.get_consultations` | **v1 (BR)** | fedlex-jolux `get_drafts` + `get_consultations` over the bridge (entry by act ELI or draft IRI; no free-text search — see deviations); **hint** |
| `get_consultation_documents` | `fedlex.get_consultation_documents` | **v1 (BR)** | fedlex-jolux `get_consultation_documents` over the bridge; **norm** |
| `get_oc_act` | `fedlex.get_oc_act` | **v1 (BR)** | fedlex-jolux `get_oc_act` over the bridge; entry by consolidation ELI only (no AS page reference — see deviations); **norm** |
| `get_memorial` | `fedlex.get_memorial` | **v1 (BR)** | fedlex-jolux `get_memorial` over the bridge, cap+1; entry by oc ELI only; **norm** |
| `get_fga_documents` | `fedlex.get_fga_documents` | **v1 (BR)** | fedlex-jolux `get_fga_documents` over the bridge; **norm** |
| `get_drafts` | `fedlex.get_drafts` | **v1 (BR)** | fedlex-jolux `get_drafts` over the bridge; unknown act → not-found; **norm** |
| `list_components` (stand-alone) | — | folded | see `fedlex.list_annexes` |
| `extract_tables` | `fedlex.extract_tables` | **v1 (BR)** | fedlex-akn `extract_tables`; header inferred from the first row when no `<th>` (Fedlex marks none), said so; caps 50 tables × 200 rows; **norm** |
| `detect_foreign_content` | `fedlex.detect_foreign_content` | **v1 (BR)** | own `xml:lang` walk (sections in another language than the manifestation) + fedlex-akn `detect_foreign_content` (`<foreign>` islands); **norm** |
| `extract_change_notes` (stand-alone) | — | folded | see `fedlex.get_modifications` |
| `parse_unlinked_ref` | `fedlex.parse_reference` | **v1 (BR)** | own parser (the vendored one reads one article label; this one reads whole citations — abbreviation, Art./Abs./lit./Ziff./Anhang, «ff.», «i.V.m.», SR, AS/BBl — and RESOLVES the act through the BO′ abbreviation pre-query); **hint** |
| `explore_node` | `fedlex.explore_node` | **v1 (BR)** | fedlex-jolux `explore_node` over the bridge, cap+1 per direction; **hint** |
| `compare_versions` | `fedlex.compare_versions` | **v1 (BR)** | own composite over two cached manifestations: articles (or one element) by their paragraphs, wording before/after capped; **norm** |
| `hollow_document`, `chunk_document` (reserved, RAG ingest) | — | never (base tier) | they feed the semantic tier's indexer — that tier's sister server, per E15's family cut |
| — (no upstream counterpart) | `fedlex.check_quote` | **v1 (BT)** | own: a quote checked against the text of the element EXACTLY as `read_article` serves it (number and heading of an article or annex, paragraph numbers and list letters included, footnotes excluded), through the cache — whitespace, quotation marks and dashes normalised, «…» splits into ordered segments; about wording only, never truth; **norm** |
| — (no upstream counterpart) | `fedlex.cite` | **v1 (BT)** | own: the canonical Fundstelle of an eId («Art. 7 Abs. 1 Bst. b LSV») — parts from the eId (the inverse of parse_reference's grammar, one vocabulary table), abbreviation from `jolux:titleShort`, SR from the taxonomy, the element verified in the manifestation; **norm** |

The count, against the upstream catalogue of 40, after BR — and two
ids beyond it since BT, where the platform goes past the reference
reader (the citation pair has no upstream counterpart): **thirty-five
ids built, thirty-three of them covering 39 upstream names** (six pairs fold
into one id each: impacts in/out, formal citations as two more
directions of that id, read_article/read_element, get_modifications/
extract_change_notes, list_components/list_annexes,
resolve_vocabulary_label/list_vocabulary); **one never**
(`get_metadata`, reason above); the two reserved RAG primitives lie
outside the 40 and belong to the semantic tier. The map is complete:
every row is v0, v1 or never-with-reason.

## The twelve BQ tools — contracts

Response fields beyond the shared `kind` + `provenance`
(`valid_as_of` = the consolidation date for the XML tools and for
`list_expressions` — the one graph tool that takes a version — and
today for the other graph tools; `transaction_time`; `source`).

### A. XML tools (one SPARQL + one manifestation fetch per call, then fedlex-akn)

1. **`fedlex.get_structure`** `{eli_version, lang?, depth?}` →
   `{structure: [{eid, kind, num, heading, children}], nodes_total,
   nodes_returned, truncated, annexes, depth, manifestation_url}`.
   `depth` = `article` (default; the skeleton, articles have no
   children) | `full`. A capped outline is a PREFIX in document order.
2. **`fedlex.search_text`** `{eli_version, query, lang?, limit?}` →
   `{hits: [{eid, element_kind, article_eid, heading, snippet}],
   total, truncated, limit}`, kind **hint**. Case-insensitive
   substring over the eId leaves; `limit` 20 default, 100 max; every
   hit names the article it sits in (`article_eid` + «Art. 17
   Kostenloser Zugang zu amtlichen Dokumenten») — the address a
   model reads next.
3. **`fedlex.read_document`** `{eli_version, lang?, max_chars?,
   offset?}` → `{markdown, total_chars, offset, max_chars, truncated,
   next_offset}`. Footnotes and formulas excluded (quotations come
   from `read_article`).
4. **`fedlex.get_references`** `{eli_version, eid?, lang?, limit?,
   offset?}` → `{references: [{source_eid, href, label}], total,
   truncated, next_offset, coverage}`, kind **hint**. `href` is the
   linked ELI where the corpus links it (work level); 15 % of refs
   carry no href — the label stays.
5. **`fedlex.get_modifications`** `{eli_version, eid?, lang?}` →
   `{change_notes: [{anchor_eid, marker, text, refs}],
   change_notes_total, truncated, mod_blocks: [{mod_eid,
   quoted_root_kind, quoted_eid, new_text}], mod_blocks_total,
   mod_blocks_truncated}` (both lists capped at 500).
   On a consolidation `mod_blocks` is empty by construction (the
   amendments are worked in); the change notes are the per-element
   record. Unknown `eid` → `not-found`.
6. **`fedlex.list_annexes`** `{eli_version, lang?}` → `{annexes:
   [{index, doc_name, eli_work, title, heading, is_empty_stub,
   eid_prefix, elements: [{eid, kind, num, heading, children}],
   elements_total, elements_truncated}], total}`. `children` is the
   COUNT of nodes below the element (the annex outline is pruned
   like `depth: article`); `elements` is capped at 200 per annex; an
   unreadable component says so in `heading`. The `eid` values are
   path eIds `read_article` reads.

### B. JOLux tools (fedlex-jolux primitives over the live graph)

7. **`fedlex.get_article_history`** `{eli, eid}` → `{target,
   impacts: [{impact_uri, date, version, type, type_label, from,
   comment}], total, truncated, completeness_note}`. `target` is the
   subdivision IRI the impacts were matched against EXACTLY (itself
   or a descendant) — three queries per call: impacts, their
   amending acts, the act's versions. `version` is the consolidation whose
   applicability date the impact opens (joined from the act's
   version list; `null` when no consolidation starts on that date).
   An empty list never proves «never amended» — the note says so.
8. **`fedlex.get_subdivisions`** `{eli}` → `{subdivisions: [{uri,
   eid, type}], total, truncated, cap, truncation_basis, note}`; an
   act the graph does not know answers `not-found`. The graph knows only
   elements with at least one amendment; the outline is
   `get_structure`.
9. **`fedlex.get_taxonomy`** `{eli}` → `{entries: [{uri, notation,
   labels{de,fr,it,en,rm}, broader}], branches: [{entry, chain:
   [root…leaf]}], truncated}`. The leaf notation IS the SR number; the chain
   climbs to the SR branch («8» for the KVG). An unclassified act
   answers empty lists with a note; an unknown act `not-found`.
10. **`fedlex.list_expressions`** `{eli_version}` → `{languages:
    [{lang, formats, xml_available, manifestations: [{format,
    url}]}], xml_available, pdf_only, no_manifestation_listed,
    manifestations_total, truncated, note}`. Three honest states: XML exists;
    files exist but no XML («PDF-only»); the graph lists no file at
    all (recorded: KVG 1996-01-01 has five language expressions and
    not one manifestation). The text tools answer `not-found` in the
    latter two — this tool says so BEFORE the read.
11. **`fedlex.resolve_vocabulary_label`** `{vocabulary, query, lang?}`
    → `{matches: [{iri, label}], returned, limit, truncated}` (label
    search; `total` for the complete IRI and language answers), kind
    **hint**. An IRI must belong to the named vocabulary.
    `query` = a label fragment (case-insensitive, any language,
    searched inside the scheme) or a vocabulary IRI (decoded to its
    label in `lang`). `vocabulary: "language"` is answered from the
    vendored official-language table without a query
    (`source_note` says so).
12. **`fedlex.find_related_topic`** `{eli?, sr?, limit?}` → `{eli,
    hits: [{eli, sr}], returned, limit, truncated, coverage}` — no
    `total`, the graph does not count for us — kind
    **hint**. Siblings under the same taxonomy parent; `sr` goes
    through `resolve_sr` first (predecessors resolved the v0 way).

## BO′ — the rest of BO on BQ, and the manifestation cache

### `fedlex.search_law` `{query, limit?}` → hints

Two ways in, one ranked list (pattern from the vendored fedlex-jolux
`search.rs`, Apache-2.0, rewritten against this backend):

1. **Abbreviation pre-query** — when the query looks like an official
   abbreviation (≤ 12 characters, ≤ 2 words): exact, case-insensitive
   match on `jolux:titleShort` of any language expression. Verified
   live and recorded: «StPO» → `cc/2010/267`, «OR» → the
   Obligationenrecht (`cc/27/317_321_377`; exactly one act carries
   «OR» as its short title). Hits rank first, `matched:
   "abbreviation"`. Fixture key `search_law:abbreviation:<query>`.
2. **Title and popular-name search** — EVERY WORD of the query, as a
   case-insensitive substring, in the SAME `jolux:title` or the same
   `jolux:titleAlternative` of a language expression (BY point 0; a
   query of more than twelve words is refused before a request). It
   was the whole query as ONE contiguous substring until the first
   live measurement asked for «Bundesgesetz über die politischen
   Rechte» and was answered the UNO covenant: the graph interpolates
   the promulgation date — «Bundesgesetz **vom 17. Dezember 1976**
   über die politischen Rechte (BPR)» — so the official title as a
   human writes it is not a substring of the title the graph holds.
   For a ONE-word query the filter is SEMANTICALLY identical to the old
   one — the fragment `all_words_in` builds is byte-for-byte what stood
   there, which is what the unit test pins; the `FILTER` line around it
   gained two parenthesis pairs (123 → 127 bytes). That is why the five
   recorded one-word windows still stand: what they recorded is what
   this query asks. For more words the match set can only grow, never
   shrink. The candidate
   window (at least 40, at most 100 acts, one beyond for a measured
   `truncated`) is ordered in-force-first BEFORE the limit cuts.
   Fixture key `search_law:<query>:<limit>` — a key may therefore
   carry spaces, and `INDEX.txt`'s `<file> <key> <recorded>` is read
   as «first token, last token, the rest between».

Ranking (client-side): abbreviation hits · in force
(`enforcement-status/0`) · a status-less stub last · the systematic
collection's own order (fewer digits in the SR number = the more
fundamental act: 832.10 before 832.102; within one depth ascending:
832.10 before 832.12) · the newer ELI. Response: `{query, hits:
[{eli, title, title_lang, titles{de,fr,it,…}, abbreviation,
abbreviations, status, in_force, sr, matched}], returned, found,
limit, truncated, abbreviation_tried, hint?}`, kind **hint**. `sr`
comes from the taxonomy joined in the same query (cheap on ≤ 100
candidates). An empty result carries `hint`: search_law is not a
full-text search and knows no synonyms — find the act by SR or a
title word, then `search_text` inside it.

Recorded before/after: «krankenversicherung» (limit 5) answered five
repealed ordinances of 1965–1987 under v0; now the KVG
(`cc/1995/1328_1328_1328`, SR 832.10) stands first. «datenschutz» →
the nDSG (`cc/2022/491`) first.

### `fedlex.check_in_force` — `future_as_of`

A Stichtag after the server's today answers from the decided
consolidations the graph already carries and says `future_as_of:
true` — a projection, never a finding. Otherwise `false`.

### The manifestation cache (live backend)

`backend::ManifestationCache`: a bounded in-process LRU (default
64 MiB, 256 entries; `Backend::live(endpoint)` builds it) keyed by the
SAME semantic key the fixture uses (`manifestation:<version>:<lang>`),
holding the resolved URL, the body and the moment of the real fetch.
A hit answers the whole chain (no resolution query, no fetch); a body
beyond the byte cap is never stored; least recently USED goes first;
nothing is persisted. Fixtures and Recording never cache.

**Provenance, extended not changed** (TOOLSET-v0: «cache serving is
marked in transaction_time semantics, not hidden»): every XML-tool
answer carries `provenance.served: "live" | "cache" | "fixture"`, and
`transaction_time` is the moment the manifestation was REALLY
retrieved (RFC 3339, UTC) — a cache hit keeps the ORIGINAL retrieval
moment instead of claiming today. The three v0 fields keep their
names and meaning; fixtures keep the injected date. Proven by
counting (`Backend::Counting`, the test double of Live): two reads of
one version + language = one fetch, one resolution query; the second
says `cache` with the first's moment; another language is another
key; eviction beyond the caps fetches again.

**Weight stays 2** (E11: the budget weight is a ceiling; cheaper is
allowed). The reference prototype's separate cache-served class is
still not claimed — the same tool may answer live or from cache, and
says which.

## BR — wave 2: the research-critical tools, and the holdings beyond the SR

### A. Research-critical

1. **`fedlex.extract_tables`** `{eli_version, eid?, lang?}` →
   `{tables: [{context_eid, rows, cols, header, header_inferred, data,
   rows_total, rows_returned, truncated, oversized}], total, returned,
   truncated}`, kind **norm**. Fedlex marks no `<th>`: when no header
   row is marked, the first row is the header and `header_inferred`
   says so. Proven on the LSV (Anhang 3 Ziff. 2, «Belastungsgrenzwerte»:
   header «Empfindlichkeitsstufe … | Planungswert Lr in dB(A) | …»,
   seven columns, stage I → 50/40/55/45/65/60).
2. **`fedlex.parse_reference`** `{text}` → `{references: [{raw, kind:
   article|annex|sr|as|bbl|unknown, abbreviation, act: {eli, sr,
   title, in_force, status} | null, unresolved, article, paragraph,
   letter, number, annex, following, sr, memorial, eid_candidate,
   annex_hint, next}], total, returned, truncated}`, kind **hint**.
   Separators: «i.V.m.», «in Verbindung mit», «;», «sowie». Paths in
   the manifestations' own spelling: `art_25_a/para_1_bis`,
   `art_3/para_1/lbl_c/lbl_2`. The act is resolved through the shared
   abbreviation pre-query (`search_law:abbreviation:<abbr>`) or, for
   «SR …», through `resolve_sr`; an abbreviation the graph does not
   know answers `unresolved: true` with a `next` step. The citation
   table of fourteen spellings is the test.
3. **`fedlex.get_citations`** gains the directions `cites` and
   `cited_by` (formal citation graph, act level, via the vendored
   JLX-CIT-01: two WAF-safe queries for `cites`, one for `cited_by`);
   `in` and `out` (the impact graph) stay as they were. ONE capability
   id with a typed direction — the v0 deviation, kept: a second id for
   the same question would double the surface.
4. **`fedlex.compare_versions`** `{eli, from_version, to_version,
   eid?, lang?}` → `{from: {version, date, served, transaction_time},
   to: {…}, eid, compared, unchanged, added, removed, changed: [{eid,
   num, heading, units: [{eid, change: added|removed|changed, before,
   after, before_truncated, after_truncated}]}], changes_total,
   truncated, granularity}`, kind **norm** (provenance of the `to`
   side). Versions as IRIs or dates. Both manifestations go through
   the cache. Proven on the BGÖ 2023-09-01 → 2023-11-01: Art. 17
   re-worded, Art. 23a inserted.
5. **`fedlex.explore_node`** `{iri, limit?}` → `{outgoing, incoming,
   outgoing_truncated, incoming_truncated, limit, truncated, note}`,
   kind **hint** — a debugging view, said so; an IRI without edges is
   not-found.
6. **`fedlex.detect_foreign_content`** `{eli_version, lang?}` →
   `{foreign_language_sections: [{eid, element_kind, lang, chars,
   snippet}], sections_total, foreign_islands: [{context_eid, kind,
   element_count}], islands_total, truncated}`, kind **norm**. The
   BGÖ, KVG, LSV and EMRK manifestations mark neither — the honest
   zero; the mechanics are proven on a synthetic document in the
   crate's unit tests.

### B. Beyond the SR

7. **`fedlex.find_treaties`** `{query?, country?, bilateral?, limit?}`
   → `{hits: [{process, title, titles, signature_date}], returned,
   limit, truncated}`, kind **hint**; at least one filter.
8. **`fedlex.get_treaty_info`** `{eli, lang?}` → `{treaty: {process,
   title, signature_date, signature_place, bilateral, party_countries,
   approbation_act}}`, kind **norm**.
9. **`fedlex.get_consultations`** `{eli?, draft?, status?, limit?}` →
   `{drafts_considered, drafts_total, drafts_truncated, consultations:
   [{consultation, draft, title, status, start_date, end_date,
   institution}], total, returned, truncated, note}`, kind **hint**.
   Since BS an own query in the vendored JLX-GEN-02 shape that walks
   BOTH paths the graph carries — `draftHasTask` (the consultation
   dossier's own draft) and `draftHasLegislativeTask` → task →
   `legislativeTaskHasResultingLegalResource` (the parliamentary draft
   `get_drafts` resolves) — with the German title; dates and
   institution from the sub-tasks (opening phase first).
10. **`fedlex.get_consultation_documents`** `{consultation}` →
    `{documents: [{document, role, kind, title}], total, under_tasks,
    opinions, truncated, truncation_basis, note}`, kind **norm**.
    Since BS two queries: the documents under the consultation's
    sub-tasks (`role: draft` = the Vorlage, `opinionIsAboutDraftDocument`;
    `role: related` = report, cover letters, address list, result
    report, `opinionHasDraftRelatedDocument`; cap+1 at 200), then the
    vendored `isOpinionOf` shape (`role: opinion`, position statements
    and result publications).
11. **`fedlex.get_oc_act`** `{eli}` → `{oc, publication_date, genre,
    genre_label, responsible_office, memorial}`, kind **norm**; an oc
    ELI as input is refused with the pointer.
12. **`fedlex.get_memorial`** `{eli (oc), limit?}` → `{memorial, acts,
    returned, limit, truncated}`, kind **norm**; a cc ELI is refused
    with the pointer to `get_oc_act`.
13. **`fedlex.get_fga_documents`** `{eli}` → `{documents: [{document,
    genre, genre_label, publication_date}], total, truncated,
    truncation_basis}`, kind **norm**.
14. **`fedlex.get_drafts`** `{eli}` → `{drafts: [{draft, draft_id,
    parliament_draft_id, resulting_resources}], total}`, kind **norm**.

Recorded data reality at BR: the nDSG and the EnG each answer ONE
draft; the EnG two Federal Gazette documents, the nDSG none; the
consultation path (draft → `jolux:draftHasTask` → `jolux:Consultation`,
the vendored J10/J11 shape) answered an empty list for both drafts —
recorded as such, not worked around. **Closed at BS** (live probe,
recorded): the parliamentary draft reaches its consultation through a
legislative task, not through `draftHasTask` — the nDSG answers the
2016/17 consultation of the DSG revision (`eli/dl/proj/6016/61/cons_1`,
2016-12-21 → 2017-04-04, status `consultation-status/5`) with 14
documents under its sub-tasks; the vendored `isOpinionOf` shape
answers 0 for it; the EnG's draft answers 0 on both paths — data
reality, said in the answer's note. The AS chain of the BGÖ:
`eli/oc/2006/355`, memorial `eli/collection/oc/2006/24` with 15 acts.
No AS/BBl document is fetched — the tools answer metadata from the
graph, so no second egress host arises.

## Deviations + grounds (this wave)

- **`get_modifications` carries the change notes.** The assignment
  named the tool «Änderungsvermerke je Element»; in the upstream
  reader that content is `extract_change_notes`, while
  `get_modifications` is the `<mod>` extraction that is empty on
  every consolidation — the input this server's `eli_version`
  denotes. One tool with both fields serves the question asked and
  keeps the upstream name; the stand-alone `extract_change_notes` is
  therefore folded, not deferred.
- **Own queries where the vendored primitive is narrower — or
  wrong — for the tool** (`get_taxonomy`: all labels + notation +
  branch chain in one query; `list_expressions`: manifestations of
  ONE version; `get_article_history`: the vendored JLX-IMP-02 matches
  the eId as a SUBSTRING, so `art_2` also collected `art_20`…`art_23a`
  — the own pair keeps upstream's WAF-safe shape with an exact
  target) — all use the predicates the vendored primitives use,
  stated in the doc comments; everything else binds the vendored
  primitives through the `KeyedClient` bridge (`backend.rs`), so
  their query shapes and rulebook knowledge come along.
- **No second egress host.** The assignment expected the
  manifestation host `www.fedlex.admin.ch`; the recorded
  manifestations all live under `fedlex.data.admin.ch/filestore/…`.
  The declaration stays exhaustive at one host and the server now
  ENFORCES it (`backend::MANIFESTATION_HOST`).
- **Weight 2 for every fedlex tool, XML tools included** — also
  after the BO′ cache: the weight is a ceiling, a cache hit is
  cheaper underneath it and says so (`served: cache`); the reference
  prototype's separate cache-served class is not claimed.

- **BR — `get_consultations` takes an act or a draft, not a free-text
  query.** The assignment sketched `{query?, status?, limit?}`; the
  graph indexes consultations per draft (J10/J11), and the vendored
  primitives walk exactly that path. A title search over
  consultations would need a predicate nobody has verified live —
  refused rather than invented. `status` filters the recorded status
  IRI. **BS — own queries instead of the vendored consultation
  primitives:** the vendored `get_consultations` walks `draftHasTask`
  only, which the parliamentary draft never carries; the vendored
  `get_consultation_documents` walks `isOpinionOf` only, which the
  documents under the sub-tasks never carry. The server asks its own
  queries in the vendored shape, widened to the paths the live probe
  found, and keeps the vendored document shape as the second query.
- **BR — `get_oc_act` / `get_memorial` take ELIs, not page
  references.** «AS 2020 752» is classified by `parse_reference`
  (`kind: as`, `memorial`), but no verified predicate maps an AS page
  reference to an oc ELI; the tools take the consolidation ELI (→ oc)
  and the oc ELI (→ memorial). A page-reference resolver is a
  named follow-up, not a silent guess.
- **BR — the citation graph is two directions, not a fifteenth id.**
  Assignment A.3 left the form open and named the direction form as
  the expected one; the count therefore reads 33 ids covering 39 of
  the 40 upstream names, not 34.
- **BR — `parse_reference` replaces the vendored `parse_unlinked_ref`
  rather than binding it:** the vendored parser reads one article
  label («Art. 58 Abs. 1 ParlG»); the platform needs whole citations
  with annexes, «ff.», «i.V.m.», SR and AS references, and the act
  RESOLVED — which only exists on this side (the BO′ abbreviation
  query).

## BS — the polite brake, and the consultation gap closed

Not a tool — a property of every live request. The backend's
`UpstreamThrottle` (backend.rs) is one token bucket over SPARQL
selects and manifestation fetches to `fedlex.data.admin.ch` in the
Live and Recording backends: 2 requests/s sustained, burst 4,
reservation semantics (a request without a token reserves the next
and waits, blocking, in arrival order), at most 5 s of waiting, beyond
that the typed `upstream-busy` at once with `retry_after_ms` — on both
query paths, the hand-written and the bridged one. Cache hits and
fixtures never touch the bucket. Configurable at the binaries
(`--upstream-rate`, `--upstream-burst`). Proven offline with
`Backend::counting_throttled` on a `FrozenClock` (waits recorded, not
slept): 6 calls in one second → 4 at once, 2 after 500 and 1000 ms;
20 at once → 14 admitted in order (waits up to 5000 ms), 6 refused
with 5500 ms and never sent; three cache hits after a first read take
no token. The gateway's rate limit per interval (weighted calls per
client per minute, `rate-limited` with `retry_after_ms`, HTTP 429 +
`Retry-After` on the JSON door) is the policy layer above it —
`mcp/gateway/README.md`.

The consultation gap: contracts 9 and 10 above, the data reality
paragraph, and the deviation note. Fixture keys re-recorded under
their semantic names (`get_consultations:<draft>` for the nDSG and EnG
drafts — same key, widened query, one honest re-record) and added
(`get_consultation_documents:<cons_1>:tasks` for the sub-task query;
the vendored opinion shape keeps the tool's own key
`get_consultation_documents:<cons_1>`, its meaning since BR).

## BT — the citation pair: check_quote and cite

Beyond the reference reader (no upstream counterpart — the upstream
catalogue reads and navigates, it never checks a quote or writes a
label): the chat builds the citation chain at article level, and by
E01/E16 it may hold no private capability, so the check happens on the
server where the norm text lies, as a gateway capability for every
BYO agent. The judge-free core metric the first prototype was measured
by («verified rate»), as a platform capability.

1. **`fedlex.check_quote`** `{eli_version, eid, quote, lang?}` →
   `{verified, segments: [{text, found, at?}], segments_total, eid,
   element_kind, eli_version, lang, text_length, note}`, kind
   **norm**. Both sides normalised the same way: every run of
   whitespace (no-break spaces included) → one space, trimmed; « » „ “
   ” ‟ → `"`, ‚ ‘ ’ ‹ › → `'`; soft hyphens removed; – — ‒ − and the
   no-break hyphen → `-`; case kept (a quote is verbatim or it is
   not). «…», «...», «[…]», «[...]» split the quote into segments;
   every segment must occur, in order (each search starts after the
   previous match); `at` is the character offset in the normalised
   text. The text checked is the element's text EXACTLY as
   `read_article` serves it — number and heading of an article or
   annex included, paragraph numbers and list letters included,
   footnotes excluded; an annex is served from its first level, so the
   text begins with that level's own number and heading and carries no
   «Anhang n» (BT′, corrected: «Öffentlichkeitsprinzip»
   against `art_6` verifies at offset 7, against `art_6/para_1` it
   does not). «[ ... ]» with inner spaces is wording, not an omission
   mark; an ellipsis that stands in the norm text itself (a repealed
   paragraph reads «…») is not quotable wording — both said in the
   note. A quote that is not
   there is `verified: false` with its missing segments — an answer,
   not an error; an empty quote, one of only omission marks, or one
   beyond 20 000 characters is `invalid-input`; an eId the version
   does not carry is `not-found`. Runs through the cache: a check
   after a read takes no token from the brake (proven by counting).
   Never a judgment about truth, completeness or force.
2. **`fedlex.cite`** `{eli_version, eid, lang?}` → `{label,
   designation: article | transitional-provision, short, sr, article,
   paragraph, letter, number, annex, title: {de, fr, it}, eli,
   eli_version, valid_as_of, lang, eid (the element actually read),
   element_kind, heading, note}`, kind **norm**. The parts come from the eId path — the
   inverse of `parse_reference`'s grammar, ONE vocabulary table for
   both directions (`de` Art./Abs./**Bst.**/Ziff./Anhang — «Bst.» is
   what the Fedlex texts and the Chancellery's Gesetzestechnische
   Richtlinien write, «lit.» is read but never written (BT′); `fr`
   art./al./let./ch./annexe, `it` art./cpv./lett./n./allegato):
   `art_25_a` → «25a», `para_1_bis` → «1bis», `lbl_b` → letter,
   `lbl_f_bis` → «Bst. fbis» (Latin suffixes both ways), a numeric
   `lbl_2` below it → number, `annex_3` → annex, `annex_u1` → «Anhang»
   alone with `annex: null`; a bare `para` (an article's single
   paragraph) and `lvl_*` add nothing; an annex label carries
   `designation: annex`, an article label `designation: article`, and an
   annex wrapper (`annex_3`) is resolved to its FIRST level as the
   manifestation orders them — never to an assumed `lvl_u1` (BT′).
   A transitional provision (`disp_u<n>`) is cited by its heading,
   verbatim, the abbreviation appended (`designation:
   transitional-provision`); a paragraph, letter or number BELOW it is
   written in the article branch's own grammar — `disp_u5/para/lbl_a` →
   «Übergangsbestimmungen zur Änderung vom 21. Dezember 2007
   (Spitalfinanzierung) Bst. a KVG» — so two places never share one
   label (BT′). `parse_reference` has no grammar for such a label, said
   in the note; a provision without a heading is refused with that
   reason. A structural element (`sec_`, `chp_`, `chap_`, `part_`,
   `book_`, `tit_`, `title_`, `preamble`, `body`, `main`,
   `conclusions`) is refused as one; any other eId shape is refused as
   «no citation grammar yet», and the refusal names the OFFENDING
   segment rather than the first. The
   abbreviation is `jolux:titleShort` of the language's expression, the
   SR number the taxonomy notation (as `resolve_sr` reads it), both
   from one query on the abstract (fixture key `cite:<abstract-eli>`);
   where the graph carries no abbreviation in that language the label
   ends in «(SR …)». The element must exist in the manifestation
   (through the cache) — a label for a place that is not there would
   be a hint, and this is a norm: `not-found` otherwise; an eId that
   names no citable place (a section, a chapter) is `invalid-input`.
   Round trip proven: `cite` → label → `parse_reference` → the same
   eId and act, on twelve addresses (two with a suffixed letter); an
   annex label parses back to its annex prefix. An annex WRAPPER
   (`annex_3`, `annex_u1` — what the hint names) is not an element the
   manifestation addresses, only its levels are; `cite` and
   `check_quote` resolve the wrapper to the first level (`annex_3/lvl_u1`,
   the annex body) and answer with the eId they actually read — so the
   hint's prefix works as an address and the answer never claims a
   place that is not there (BT′, decided and tested).

Deviation from the assignment, said: the BGÖ's Art. 23a has ONE
unnumbered paragraph in the recorded manifestation (`art_23_a/para`),
so «Art. 23a Abs. 2 BGÖ» names no place there; the label test uses
`art_23_a` and `art_23_a/para` → «Art. 23a BGÖ» and the KVG's
`art_25_a/para_1/lbl_b` → «Art. 25a Abs. 1 Bst. b KVG» for the
suffixed-article-with-paragraph case.

## BV — what the rulebooks changed in the contract

Seven answers changed shape when the server was held against the two
verified Fedlex rulebooks (`docs/reference/fedlex-data-rules.md`, the
conformance table and its gate). The tools are the same thirty-five;
what they promise is now what the data supports.

1. **`check_in_force`** answers from the ACT's own dates (vendored
   JLX-TMP-03 over the bridge): `{in_force, as_of, dates: {entry_in_force,
   no_longer_in_force, end_applicability}, status, status_label,
   status_unset, no_enforcement_data, governing_version, future_as_of}`.
   It started when the entry date is not after the day and has not
   ended when neither end date is — the EARLIER of the two counts
   (J3.2: they disagree on about 4 % of expired acts). The governing
   consolidation stays beside the answer and no longer decides it.
   `no_enforcement_data: true` means the graph knows neither status nor
   date — «false» is then «no data», not «out of force» (J3.3).
2. **`list_versions`** answers an EMPTY list with its reason for an act
   the graph knows and never consolidated (6'532 acts are in that
   state); only an ELI the graph knows nothing about is a `not-found`.
3. **`get_law_metadata`** carries `status_label` (`skos:prefLabel`, de
   → en/fr/it), `status_unset`, `in_force` by the same rule as (1), and
   the two end dates beside the entry date. **`resolve_sr`** carries
   them on the chosen act and on every `also_matches` row (J5.3).
4. **`get_citations` `in|out`** keeps the two directions and drops the
   promise: the field is `jolux:foreseenImpactToLegalResource` — 0.8 %
   of the impact graph, no type, no date, and at the incoming end
   mostly the consultation drafts that foresee an impact (the recorded
   KVG answer is 33 of them). «Who amended this act» is
   `get_article_history`, «who cites it» is `cited_by` (J16.1).
5. **`read_document`, `get_structure`, `search_text`** no longer lose
   text that sits directly under `<body>` — a signature block, a stray
   paragraph or table (X17.8/X18.7). `read_document` renders it in
   document order, `get_structure` names the elements with their length
   and says they carry no eId, `search_text` finds them and answers
   `eid: null` with the element's tag.
6. **`check_quote` and `cite`** carry `eid_duplicates` and
   `eid_via_normalisation` like `read_article` (X15.3), and their notes
   say what a duplicate means for the answer.
7. **The live requests** carry two timeout classes, chosen against the
   caller's patience: 15 s for a SPARQL select (the chat allows a tool
   call 15 s, the brake may reserve 5 of them), 30 s for a manifestation
   fetch. The refusal names the class and the limit (J17.5).

New fixture keys: `check_in_force:<eli>` (the act-level check, one per
act); the changed queries of `get_law_metadata` and `resolve_sr` were
re-recorded under their own names on 2026-08-29.

## Fixture keys (tests/fixtures/INDEX.txt)

`manifestation:<version>:<lang>` and `manifestation:xml:<version>:<lang>`
are shared by every XML tool (the manifestation is a property of the
version, not of the tool — the two v0 `read_article:` keys were
renamed to this shape at BQ without re-recording). JOLux keys are
`<tool>:<params>`; a vendored primitive that asks twice gets `:q2`,
the own article-history pair `…:sources`; search_law's abbreviation
pre-query `search_law:abbreviation:<query>`; the act profile behind
`cite` is `cite:<abstract-eli>`. A key whose query was
rewritten keeps its name and file and is re-recorded — INDEX.txt
lists such replacements at its end.
