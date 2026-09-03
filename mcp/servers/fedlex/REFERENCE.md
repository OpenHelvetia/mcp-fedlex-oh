# fedlex reference behavior (input for the L2.2 rebuild)

<!-- language: English -->

**Spec only — no server code in this package.** This document records
what the reference prototype demonstrably DOES, from public surfaces,
so the platform's OWN fedlex domain server (home `mcp/servers` per
sealed E06; M2 of the sealed E04 work plan) can be built against
documented behavior. **Governance boundary (E15: reference project,
never absorption): the prototype's code belongs to ansv/mindful.bio —
behavior is documented here, code is NOT copied.** The public repo
advertises Apache-2.0 (see below); the corpus rule stands regardless:
any code transfer would need Alex's explicit, documented license act
(open governance input, report §28).

## Method and provenance (retrieved 2026-08-20/21, polite single requests)

- `https://fedlex.ansv.ch` — public demo page (HTTP 200; WebFetch was
  403, plain GET with an honest UA succeeded).
- `https://fedlex.ansv.ch/mcp` — POST tools/list (stateless rmcp
  `_meta` form): **HTTP 405** — the endpoint path from the corpus
  example manifest no longer accepts this method.
- `https://mcp-fedlex.ch/mcp` — same single request: **HTTP 401** —
  the MCP surface is credential-gated (fail-closed, see below). No
  further probing; the public documentation pages carry the full
  catalogue and were used instead.
- `https://mcp-fedlex.ch` + `/werkzeuge` — documentation pages
  (HTTP 200).
- Ground truth (S3, retrieved 2026-08-20T21:28:56Z): SPARQL ASK on
  `https://fedlex.data.admin.ch/sparqlendpoint` for
  `jolux:ConsolidationAbstract` → `"boolean": true` (endpoint live,
  JOLux vocabulary `http://data.legilux.public.lu/resource/ontology/jolux#`
  confirmed in use); ELI resolution
  `https://fedlex.data.admin.ch/eli/cc/1999/404` with
  `Accept: text/turtle` → HTTP 200, 11379 bytes, redirected to a
  CONSTRUCT over the named graph `…/eli/cc/1999/404/graph`, Turtle
  carries the `jolux:` prefix (e.g.
  `jolux:SubdivisionIdentificationDetail` on `…/art_10a/1` — eId-level
  resources exist in the official graph).

## What the demo site shows (verbatim feature names)

`fedlex.ansv.ch — inoffizielle Fedlex-Demo`: «Publikationsplattform
trifft KI-Assistent. Das Bundesrecht, belegkettengestützt befragbar.»
Sections: **Suche · Explorer · Benchmark · Fedlex-Spiegel ·
mcp-fedlex.ch**.

- **Analyse-Demo:** a legal question is answered by an agent with a
  visible agent terminal; the product is a «Gutachten mit Belegkette,
  jede Quelle eId-genau verlinkt und mechanisch verifiziert» —
  citation chains anchored at eId precision, mechanically verified.
- **Fedlex-Spiegel:** the official Fedlex UI mirrored live with an
  embedded chat assistant on every page (content unchanged from
  fedlex.admin.ch, reuse per the federal redistribution terms).
- **Gesetzes-Explorer:** federal law as a relationship graph —
  «Erlass suchen, die systematische Einordnung sehen, Änderungen und
  Verweise Kante für Kante verfolgen — direkt aus den offenen
  JOLux-Daten».
- **Offene Infrastruktur:** «der offene mcp-fedlex-Server —
  Navigations-Werkzeuge über dem amtlichen Datenbestand, dokumentiert
  und frei nutzbar» → mcp-fedlex.ch.

## The mcp-fedlex tool surface (from its public documentation)

Identity: «Der provenance-gesicherte MCP-Server für Schweizer
Bundesrecht» — Rust, repo `github.com/mindful-bio/mcp-fedlex`,
license advertised Apache-2.0, author mindful.bio GmbH.

**Three guarantees (their wording, condensed):** (1)
**Provenance-Pflicht** — every content answer carries origin: ELI,
`valid_as_of` (which consolidation governed at the reference date),
`transaction_time` (when retrieved); a tool without provenance serves
no norm content. (2) **Stichtagsgenauigkeit** — every query may carry
`as_of` and receives exactly the version in force that day;
bitemporal, reproducible, auditable. (3) **Fail-closed** — no valid
credential, no answer; identity (tenant, session, role) comes only
from the signed JWT, never from model-settable parameters; every tool
call writes an audit log line through an allowlist PII scrubber.

**Functional space:** 40 registered tools = a curated projection from
a 47-primitive capability lexicon (`10_LEXICON_jolux.md` for JOLux
metadata, `11_LEXICON_akn.md` for the Akoma-Ntoso act text), each
primitive individually conformance-tested live against Fedlex;
39 projected + 8 reserved (6 internal building blocks + 
`hollow_document`/`chunk_document` for the RAG ingest of the sister
component **mcp-fedlex-semantic**) + 1 composite (`compare_versions`).
An offline test (`lexicon_projection.rs`) keeps lexicon and tool
surface congruent in CI. Roles project pools: reader ⊆ navigator ⊆
validator. Answers distinguish **Norm-Beleg** (`kind: "norm"`,
citable) from **Discovery-Hinweis** (`kind: "hint"`, a candidate
until a norm proof confirms it).

### Catalogue (40 tools, by pool — tier mapping per E16)

**Pool LocalNavigation** (reader, 13 — served from a manifestation
cache; tier: **base**): `read_article`, `read_element`,
`read_document`, `get_structure`, `search_text`, `get_metadata`,
`get_references`, `get_modifications`, `list_components`,
`extract_tables`, `detect_foreign_content`, `extract_change_notes`,
`parse_unlinked_ref`.

**Pool Discovery** (navigator, 10 — live against the public Fedlex
SPARQL endpoint, hint provenance; tier: **base**): `search_law`,
`resolve_sr_number`, `find_related_topic`, `find_treaties`,
`get_treaty_info`, `get_consultations`, `get_consultation_documents`,
`resolve_vocabulary_label`, `list_vocabulary`, `explore_node`.

**Pool JoluxMetadata** (navigator, 16 — live SPARQL; tier: **base**):
`check_in_force`, `list_versions`, `resolve_consolidation_at`,
`get_impacts`, `get_outgoing_impacts`, `get_article_history`,
`get_citations`, `get_taxonomy`, `get_subdivisions`, `list_annexes`,
`get_law_metadata`, `list_expressions`, `get_oc_act`, `get_memorial`,
`get_fga_documents`, `get_drafts`.

**Pool Validation** (validator, 1; tier: **base**):
`compare_versions`.

**Tier mapping (E16 tier principle):** all four pools are stateless
ELI-graph navigation/retrieval/citation → **base**. The
**semantic** tier is the sister component mcp-fedlex-semantic
(embedding search; fed via the reserved `hollow_document`/
`chunk_document` primitives; corpus example manifest names the
`/mcp-semantic` endpoint). The **generative** tier is the ansV
analysis layer (Gutachten with citation chains — the Analyse-Demo),
not a tool of the base server.

**Response shape** (documented): every successful answer is
`{ "data": …, provenance… }` — payload plus origin, uniformly.

**Quota note** (documented): Discovery/JoluxMetadata weigh heavier in
the quota than LocalNavigation (cache-served) — a live-SPARQL
cost-awareness the rebuild should keep.

## Corpus lineage

- E15 (sealed): the fedlex family — «mcp-fedlex navigieren ·
  mcp-fedlex-semantic suchen, samt Indexer · mcp-fedlex-skills
  schliessen; verbunden im Agenten ansV, der alle drei mountet —
  fedlex.ansv.ch ist das [Schaufenster]» — is the FIRST INSTANCE of
  the engine pattern and the reference project (E21 clause 2:
  reference project no. 1).
- E16/research.md: the «proven fedlex.ansv.ch pattern» (Oxigraph WASM
  explorer over static dumps) informs the platform's own read-only
  stack recommendation.
- Corpus example manifests (`registry/standard/examples/
  fedlex-ansv-mcp.json`, test corpus `fedlex-jolux-sparql.json`)
  carry the endpoints and probe hints; the `/mcp` example path
  405s today (finding above) — the example's pre-publication caveat
  («verify endpoint details before publication») is confirmed
  necessary.

## Where the catalogue landed (BQ, 2026-08-29)

The 40-tool catalogue above is mapped tool by tool onto this server's
capability ids in **TOOLSET-v1.md** (upstream name → `fedlex.<id>` →
status v0 | BQ | BR | never, with the reason). State after BQ wave 1:
20 built (the v0 spine plus the navigator surface: outline, in-act
search, whole document, references, amendment notes, annexes,
article history, subdivisions, taxonomy, language versions,
vocabulary, related acts), then wave 2 at BR (tables, citation parsing,
formal citations, version comparison, node view, foreign content,
treaties, consultations, drafts, the Official Compilation and the
Federal Gazette): 33 ids covering 39 of the 40 names, 1 never
— plus, since BT, two ids beyond the catalogue (check_quote, cite) —
(get_metadata; six upstream pairs folded into built tools, two reserved
primitives belong to the semantic tier's ingest). The quota note above is
honoured in its direction, not its split: every tool weighs the same
(2) at the gateway; since BO′ the XML tools serve repeat reads of one
version from a bounded in-process cache and SAY so in the provenance
(`served: cache`, the original retrieval moment) — a cheaper answer
under the same ceiling, not a separate cache-served class.
