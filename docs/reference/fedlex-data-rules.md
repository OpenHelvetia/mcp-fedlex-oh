---
title: Fedlex data rules — the rulebooks as a conformance table
type: Reference
status: informative (reference); the conformance table is gated by a test
language: English
updated: 2026-08-30
owner: Jonathan
review-by: 2026-12-31
maintenance: a rule changes only when the data changes; a status changes with every wave that touches the server
---

# Fedlex data rules — rule, tool, test, status

<!-- language: English -->

**Provenance:** derived from the author's Fedlex data-understanding
analyses, 2026-06, adopted by consent. The rules were verified there
against 15'807 Akoma-Ntoso files and the live JOLux endpoint; the
identifiers (`J…`, `X…`) are kept so the two bodies of work stay
navigable against each other. The restatement is this corpus's own —
English, shortened to what a base-tier tool must know.

**What this page is for.** The first domain server's hardest defects
were DATA defects, and every one surfaced during a build (consultations
answering empty, the subdivision gap catalogue, the substring match on
`art_2` that collected `art_20`). The rules that would have predicted
them existed before the server did. This page states them, and — §4 —
says for each one whether the server honours it, contradicts it, or
merely happens not to be tested. A test parses that table
(`tests/rules_table.rs`): a row that claims `honoured` must name a test
function that exists.

## 1. How to read an entry

Every entry carries four things: the **statement** (what the data
does), the **figure** it rests on (restated from the analysis), the
**consequence for a tool** (what a base-tier tool must therefore do),
and — in §4 — the tool, the test and the status.

The five statuses mean exactly this, and nothing else:

| status | meaning |
|---|---|
| `honoured` | a test in this crate pins the behaviour the rule requires; the test is named and it passes |
| `violated` | the code provably contradicts the rule; the note carries the concrete case |
| `untested` | the code plausibly honours it and no test pins it — «it looks right» is this, never `honoured` |
| `not_applicable` | outside the base tier or outside the served scope |
| `unknown` | not settled; the note says what was looked at |


## 2. JOLux — the metadata graph (J0–J20)

### J0.1 — JOLux carries metadata, the text lies in the XML

JOLux is the RDF metadata model of Swiss federal legislation. It describes structure, relations and metadata of acts — never the legal text itself. The authentic wording lives exclusively in the Akoma-Ntoso manifestation.

**Figure:** 0 % of the legal text is in JOLux; 100 % is in the XML.

**Consequence for a tool:** A tool that answers wording must read the manifestation; a tool that answers facts about an act may stay in the graph. The two families must not be mixed in one answer without saying which side spoke.

### J0.2 — One endpoint, four namespaces

The SPARQL endpoint is `https://fedlex.data.admin.ch/sparqlendpoint`; the vocabularies are the JOLux ontology namespace, SKOS, and the ELI base `https://fedlex.data.admin.ch/eli/`.

**Figure:** One endpoint for every collection.

**Consequence for a tool:** One declared egress host suffices for the whole domain — the manifestation files live under the same host.

### J0.3 — The graph is deep, not wide

One act produces roughly 90 direct edges but 200+ nodes: consolidations × expressions × manifestations, plus subdivisions and impacts. The complexity is in the depth, not in the number of predicates on the top node.

**Figure:** ~90 edges, ~200 nodes, 13 versions for the EnG.

**Consequence for a tool:** A tool must walk the chain rather than read one node; a node view is a debugging help, not an answer.

### J1.1 — Nineteen classes

The model carries 19 classes: the CC core (ConsolidationAbstract, Consolidation, Expression, Manifestation, Work), the Official Compilation's Act, impacts, subdivisions, citations, six consultation classes, drafts and three treaty classes.

**Figure:** 19 of 19 verified against the live endpoint.

**Consequence for a tool:** Every tool family of the base tier maps onto one of these classes; a tool for a class outside them would be inventing.

### J1.2 — Sixty-five predicates, fourteen of them on the act node

Of 65 predicates only 14 appear directly on a `ConsolidationAbstract`; the other 51 sit on linked nodes. `legalResourceGenre` and `responsibilityOf` are empty on the CC level and filled on the OC act level.

**Figure:** 0 of 69'350 CC acts carry a genre; 99.6 % of OC acts do.

**Consequence for a tool:** `get_law_metadata` must not promise genre or responsible office for a consolidated act; `get_oc_act` may.

### J2.1 — The FRBR chain runs INTO the abstract act

The way from the abstract act to the text is `?consolidation jolux:isMemberOf <abstract>` → `isRealizedBy` → `isEmbodiedBy` → `isExemplifiedBy`. The relation abstract → consolidation is incoming; querying it outgoing returns nothing.

**Figure:** The outgoing direction answers 0 rows.

**Consequence for a tool:** The version resolution query must be written in the incoming direction, or every read fails.

### J2.2 — The newest version is one ordered query

The governing manifestation is found by ordering the consolidations of an act by `dateApplicability` and taking the newest that is not after the reference date.

**Figure:** One query per version, LIMIT 1.

**Consequence for a tool:** A tool must resolve the version server-side and stamp what it resolved, never ask the caller to pick.

### J2.3 — All three collections carry XML, not every entry does

CC, OC and FGA all have XML manifestations; an individual entry may carry only DOC or PDF. XML exists broadly from about 2021.

**Figure:** 3'409 CC, 3'204 OC, 9'193 FGA XML files downloaded.

**Consequence for a tool:** A tool must be able to say «this version has no XML» as an answer, and show which formats exist.

### J3.1 — dateEntryInForce is the working start date

`dateEntryInForce` carries the start of legal effect for practically every act; `dateApplicability` on the act level is a phantom. `dateNoLongerInForce` carries the end for 96 % of expired acts.

**Figure:** `dateApplicability` on the act level: 3 of 69'186 acts (0.004 %).

**Consequence for a tool:** A tool that answers «from when» reads `dateEntryInForce`; `dateApplicability` belongs to the consolidation, not to the act.

### J3.2 — Validity needs both end dates

An act is in force when it carries no end date or an end date in the future. In about 4 % of expired acts `dateNoLongerInForce` and `dateEndApplicability` disagree, so a precise answer checks both.

**Figure:** 4 % of expired acts diverge; SR 916.350.18 is the named case.

**Consequence for a tool:** A tool that answers «is this still in force» must read the end dates it has and say which one it used.

### J3.3 — Fifteen per cent of the acts carry no in-force status

Only three of six status codes are used (in force, no longer in the SR, no longer in force). 10'479 acts carry no `inForceStatus` at all, spread over every document type and 1850–2026.

**Figure:** 10'479 of 69'391 acts (15.1 %) without a status.

**Consequence for a tool:** Every status query must be OPTIONAL, or a sixth of the corpus disappears from the answer.

### J3.4 — titleShort is unreliable

The official abbreviation `titleShort` is often absent; it can be recovered from the full title's parenthesis.

**Figure:** Absent on a large share of the acts — the reason the analysis calls it unreliable.

**Consequence for a tool:** A tool must not fail when the abbreviation is missing; it needs a fallback and must say which it used.

### J3.5 — The title carries the promulgation date in its middle

The official German title is not «type + subject»: the graph interpolates the promulgation date between the two — «Bundesgesetz vom 17. Dezember 1976 über die politischen Rechte (BPR)», never «Bundesgesetz über die politischen Rechte». The subject follows «über», or «betreffend» in the older acts, and the date stands in front of it. The title as a human writes it is therefore not a substring of the title the graph holds.

**Figure:** 77 of the recorded German titles name their subject with «über» or «betreffend» and carry a one-word act type in front of it; 73 of those carry «vom ‹Datum›» between the two. Measured by the suite, not by hand — `tests/e2e.rs::the_promulgation_date_census_of_the_recorded_titles` walks the fixtures and asserts the figure, the way X6.1's line-break census does, because two hand counts of this corpus were written down with two different numbers and neither was reproducible. The four exceptions: the Forstpolizei act of 1902 (SR 921.0) and its predecessor of 1876, both left dateless by the graph; the act shell without a consolidation (`cc/2020/2930_cc`, the DSG, whose consolidated form does carry the date); and one draft.

**Consequence for a tool:** A title search must ask for every WORD of the query in the same title, never for the query as one contiguous string — a caller who types the official title will otherwise miss the act whose title it is.

### J4.1 — JOLux knows 8.5 % of the structure at best

Only elements touched by at least one amendment exist as a subdivision. For the EnG that is 66 of 779 eIds; paragraphs, items, sections and levels are absent entirely.

**Figure:** 8.5 % of the eIds; paragraphs 0 %.

**Consequence for a tool:** A structure tool must read the XML; the graph's subdivisions are a gap catalogue and must be labelled as one.

### J4.2 — The coverage falls with the age of the act

The older and more stable an act, the less of it the graph knows: 0.4 % for the federal constitution, 3.1 % for the code of obligations, 8.5 % for the energy act.

**Figure:** 0.4 % / 3.1 % / 8.5 % measured on three acts.

**Consequence for a tool:** A tool must never present the graph's element list as the act's table of contents.

### J4.3 — What the graph does not know

Paragraphs, enumeration items, sections and levels have no subdivision at all; unchanged articles are absent too.

**Figure:** 0 % for paragraphs, items, sections and levels.

**Consequence for a tool:** Any answer below article level comes from the XML — a paragraph address cannot be validated against the graph.

### J4.4 — Twenty-four subdivision types

The graph's subdivision types include article, chapter, section, paragraph, title, part, annex, book, level, preamble, final and transitional provisions — 24 values in all, with 1'611 annexes.

**Figure:** 1'611 annex subdivisions, 1'266 of them under a consolidated act.

**Consequence for a tool:** A subdivision answer must carry the type as a vocabulary IRI, not as a guessed word.

### J4.5b — The subdivision count depends on the query

Counting subdivisions directly, transitively, with a type filter or through the consolidation path gives 68, 68, 81 or 171 for the same act. Earlier published numbers differ for exactly this reason.

**Figure:** Four methods, four counts for the EnG.

**Consequence for a tool:** A tool must say which walk produced its list, and cap it visibly, so two answers can be compared at all.

### J5.1 — Forty-six vocabularies, 49'184 entries

The graph's opaque URIs resolve through 46 SKOS catalogues holding 49'184 concepts.

**Figure:** 46 catalogues, 49'184 entries.

**Consequence for a tool:** A tool answering a vocabulary IRI must offer a way to resolve it.

### J5.3 — Resolving vocabularies is mandatory

Values like `resource-type/21` or `enforcement-status/0` are meaningless without their catalogue; every consumer needs the lookup.

**Figure:** Opaque URIs on status, type, institution, country, impact type, subdivision type.

**Consequence for a tool:** Answers may carry the IRI, but the platform must serve the resolution as a capability — not leave it to the client.

### J5.4 — Six catalogues carry no German label

All 46 catalogues resolve; six answer in another language only (the Italian and French subject themes, two ELI tables, the information source, the licence).

**Figure:** 40 of 46 with German labels.

**Consequence for a tool:** A label answer must not promise German; it returns what the catalogue has and says which languages those are.

### J5.5 — Twenty-three document types

`typeDocument` distinguishes 23 kinds of act: federal acts, federal-council ordinances, departmental and office ordinances, bilateral and multilateral international texts and more.

**Figure:** Federal acts 10.2 %, federal-council ordinances 40.3 %.

**Consequence for a tool:** A search must not assume «act»; the type belongs in the answer as a vocabulary IRI.

### J6.1 — The impact chain is the formal history

Every amendment of an element is its own node: from the amending act, to the affected element, with type, entry into force and the consolidation that carried it.

**Figure:** 306'526 impact nodes.

**Consequence for a tool:** «When and why did this article change» is answerable from the graph — the tool walks the chain rather than the text.

### J6.2 — Three source systems coexist

Impacts come from three systems: the old chronology (63.9 %), the old mutation system (26.9 %) and the new structured one (9.2 %); 2.7 % carry no source at all.

**Figure:** 190'454 / 80'206 / 27'527 impacts; 8'339 without a source.

**Consequence for a tool:** A history answer must not claim completeness — the sources differ in what they record.

### J6.3 — Free text and structure coexist in one impact

An impact may name its target as a structured subdivision, as a free-text comment, or both; 85'817 carry both.

**Figure:** 38'701 comment only, 212'362 subdivision only, 85'817 both.

**Consequence for a tool:** A history tool must read the comment as well as the subdivision, or it silently loses the free-text half.

### J6.4 — Since 2023 the free-text method dominates again

Between 2010 and 2022 the structured method prevailed; from 2023 the free-text comment dominates with more than 15'000 impacts a year.

**Figure:** The reversal is dated 2023.

**Consequence for a tool:** An article history built on subdivisions alone becomes less complete the more recent the amendment — the tool must say so.

### J6.5 — The graph is the only source for repealed articles

Articles repealed long ago exist in the graph but no longer in the current manifestation — 36 in the code of obligations.

**Figure:** 36 repealed articles in one act.

**Consequence for a tool:** An article-history answer for an eId the XML no longer carries is a legitimate answer, not a not-found.

### J7.1 — Formal citations are at text level

JOLux citations connect whole texts (`…/text`), not articles: they say that one act cites another, never where.

**Figure:** Granularity: act level, not article level.

**Consequence for a tool:** A citation answer must say that the granularity is the act — otherwise the reader takes it for an article-precise link.

### J7.2 — descriptionFrom was found systematically empty

The analysis of 2026-04 found the citation description empty on every tested citation.

**Figure:** 0 % filled at the time of the analysis.

**Consequence for a tool:** A tool may pass the field through, but must not depend on it.

### J7.3 — Formal citations and in-text references overlap by 0–48 %

The graph's citations and the references in the authentic text are different worlds: for the constitution the overlap is zero, for the code of obligations 26 %, for the energy act 48 %.

**Figure:** 265 vs 100 refs with 0 overlap; 286 vs 99 with 75; 37 vs 46 with 20.

**Consequence for a tool:** A complete picture needs both tools, and each must point at the other.

### J7.4 — Every version writes its own citation rows

Each consolidation of an act produces its own citation entries, so a raw query returns the same act many times over.

**Figure:** 242 recorded rows for 17 distinct acts (BGÖ, 2026-08-29).

**Consequence for a tool:** A citation answer must deduplicate by target and report the distinct count.

### J8.1 — The Official Compilation is the chronological half

The Official Compilation documents the single amendment as published; the classified compilation shows the consolidated state. The two complement each other.

**Figure:** Two collections, one act.

**Consequence for a tool:** A tool answering «what was published» must go to the Official Compilation, not to the consolidated text.

### J8.2 — Not every Official Compilation entry has XML

Entries of the Official Compilation carry XML only in part; some answer DOC and PDF only, and their XML is the amending act's text, never the consolidated one.

**Figure:** 3'204 XML files across the collection.

**Consequence for a tool:** For the consolidated wording a tool must always use the classified compilation.

### J8.3 — The Official Compilation reuses the CC schema

An Official-Compilation act carries the same date, type and sequence predicates as a consolidated act, plus its membership in a weekly memorial.

**Figure:** Genre filled on 99.6 % of Official-Compilation acts.

**Consequence for a tool:** The tool may answer genre and responsible office here — the fields the consolidated level leaves empty.

### J8.5 — Seven to nine thousand acts a year

The Official Compilation published 7'400–9'100 acts a year between 2016 and 2020, with a falling trend since 2021.

**Figure:** 7'400–9'100 acts a year.

**Consequence for a tool:** A memorial listing must be capped and say so — an issue can be long.

### J9.1 — The Federal Gazette is mostly context

95.2 % of Federal-Gazette entries are dispatches, reports and notices; only 8 % are acts of any kind.

**Figure:** 93'680 of 98'353 entries are «other».

**Consequence for a tool:** A tool answering the Federal Gazette must say that these are context documents, not law in force.

### J9.3 — The Federal Gazette is structurally unlike the classified compilation

A Federal-Gazette act has no consolidation timeline, no subdivisions, no impacts, no citations and no SR number — but it does carry genre and responsible office.

**Figure:** 7 of 17 CC predicates present.

**Consequence for a tool:** A tool must not offer version, history or citation answers for a Federal-Gazette document.

### J9.4 — Half of the publication universe is the Federal Gazette

By family type the corpus divides into Federal Gazette 50.0 %, Official Compilation 36.8 %, classified compilation 13.2 %.

**Figure:** 211'539 / 155'737 / 55'962 entries.

**Consequence for a tool:** Search that does not filter the family answers mostly Federal-Gazette documents — the served scope must be explicit.

### J10.1 — Two and a half thousand consultations

The graph holds 2'514 consultation procedures, 91.6 % of them completed.

**Figure:** 2'514 consultations, 2'302 completed.

**Consequence for a tool:** A consultation answer is a hint about the genesis, and its status belongs in the row.

### J10.2 — The phase model is a graph, not a ladder

A consultation reaches its phase through a task node, never directly; the dates and the institution hang on the sub-tasks. A direct query from phase to consultation answers nothing.

**Figure:** 2'514 consultations, 6'279 tasks, 2'459 phases.

**Consequence for a tool:** The tool must traverse the sub-task node for dates and institution, or answer them empty.

### J10.3 — The granular institution is a second level

`institutionInChargeOfTheEventLevel2` names the state secretariat where level one names the department, and is present almost one-to-one with the tasks.

**Figure:** 6'245 uses against 6'279 tasks.

**Consequence for a tool:** An answer that carries only level one names a department where the graph knows the office.

### J10.4 — Consultations are context, not law

A consultation answers «why is the act worded this way», never «what is in force».

**Figure:** 0 legal norms in the collection.

**Consequence for a tool:** The answer kind must be a hint, and the note must point at the acts in force.

### J11.1 — Eighty-six thousand drafts

The graph holds 85'996 draft processes reaching back to about 1848.

**Figure:** 85'996 drafts.

**Consequence for a tool:** A draft answer is the entry into the genesis, not a legal source.

### J11.2 — The parliamentary number is the bridge to Curia Vista

`parliamentDraftId` (e.g. «13.074») is the key to the parliament's own database; the legislative tasks document the process step by step.

**Figure:** Six legislative tasks for the energy act.

**Consequence for a tool:** The tool must carry the parliamentary number through — it is the only federated key the collection offers.

### J11.3 — Drafts answer «how did this come about»

Drafts concern the process of making the law, never the law in force.

**Figure:** A collection of processes, not of norms.

**Consequence for a tool:** The tool's line must say so, and the answer must not be quotable as a norm.

### J12.1 — Twenty thousand treaty processes

The graph holds 19'830 treaty processes, 96.7 % of them base treaties.

**Figure:** 19'830 processes; 337 additional and 325 amending protocols.

**Consequence for a tool:** A treaty search must be capped and say what it counted.

### J12.2 — Two thirds are multilateral

31.7 % of the treaty processes are bilateral, 66.7 % multilateral, 1.6 % unknown.

**Figure:** 6'283 / 13'222 / 325.

**Consequence for a tool:** Bilaterality is a filter worth offering, and «unknown» must survive it as a state.

### J12.4 — Treaties carry SR numbers and consolidated texts

Many treaties have an SR number under 0.xxx and exist as a consolidated act with a manifestation.

**Figure:** The 0.101 human-rights convention is the recorded case.

**Consequence for a tool:** The citation loop must work for a treaty exactly as for a domestic act.

### J13.1 — German, French and Italian are complete, English and Romansh are not

5'854 acts have German and French XML, 5'853 Italian, 290 English and 85 Romansh.

**Figure:** 5'854 / 5'854 / 5'853 / 290 / 85.

**Consequence for a tool:** A manifestation language parameter may offer all five languages; WHICH of them a version carries is the graph's answer, never the tool's rule.

### J13.2 — eIds are language-invariant

The same act carries the same eIds in every language: 779 of 779 identical across German, French and Italian.

**Figure:** 779/779 identical; the French text is 12 % longer, the Italian 8 %.

**Consequence for a tool:** One address reads in three languages — a citation chain may switch language without re-resolving the place.

### J13.3 — Text length differs by language

French runs about 12 % longer than German, Italian about 8 %.

**Figure:** 123'232 / 137'701 / 133'055 characters for one act.

**Consequence for a tool:** A character cap must be measured on the answer actually served, not assumed from the German.

### J14.1 — XML exists from about 2021

Of thirteen consolidations of the energy act eleven have XML; the two oldest exist only as PDF/A.

**Figure:** 11 of 13 versions with XML.

**Consequence for a tool:** «This version is PDF-only» is an answer a tool must be able to give, with the reason.

### J14.1b — The number of versions depends on the document type

Federal acts carry 12.3 versions on average, bilateral international texts 1.5; the maximum measured is 196.

**Figure:** 336 federal acts, mean 12.3, max 118.

**Consequence for a tool:** A version list must be capped and ordered, never assumed short.

### J14.2 — Historical versions are readable from 2021

«What was in force then» is answerable for versions from about 2021; older states exist as PDF/A only.

**Figure:** The 2021 boundary is the same for every document type.

**Consequence for a tool:** The bitemporal loop must resolve the historical version and then say honestly whether it can be read.

### J14.3 — The date filter belongs in the query

The version governing a date is found by filtering `dateApplicability` against that date inside the query, not by fetching and sorting client-side.

**Figure:** One query, LIMIT 1.

**Consequence for a tool:** The server resolves the date; the client may ask for one but never picks the version.

### J15.1 — Federal acts are a tenth of the corpus

Ordinances dominate the classified compilation: federal-council ordinances 40.3 %, departmental ordinances 14.8 %, federal acts 10.2 %.

**Figure:** 27'964 / 10'240 / 7'089 of 69'350.

**Consequence for a tool:** A search that ranks «acts» above ordinances by type would answer the wrong thing nine times out of ten; the ranking must be by the collection's own order.

### J15.2 — Sixteen hundred annexes

1'611 subdivisions are annexes, 1'266 of them under a consolidated act; they are addressable by eId.

**Figure:** 1'611 annexes, 500 acts carry one.

**Consequence for a tool:** Annexes need their own listing tool and their own address form.

### J15.3 — The oldest act is from 1830

The classified compilation reaches back to 1830 (a cantonal constitution).

**Figure:** 1830–2026.

**Consequence for a tool:** Date handling must not assume a modern range.

### J16.1 — Six fields are ignorable

The foreseen impact (0.8 %), the act-level applicability date (0.004 %), the end-of-applicability date (4 %), the citation description, the short title and the Dublin-Core identifier carry little or nothing.

**Figure:** 2'598 of 306'520 impacts are «foreseen».

**Consequence for a tool:** A tool must not build an answer on them — and where it exposes one it says how thin it is.

### J16.2 — The cogni predicates do not exist publicly

Internal predicates seen in some dumps answer nothing on the public endpoint.

**Figure:** 0 hits, verified 2026-04.

**Consequence for a tool:** Nothing may be built on them.

### J16.3 — Publication completeness is nearly total

97.4 % of the publications are complete; 2.6 % publish text by reference.

**Figure:** 20'954 of 21'517 complete.

**Consequence for a tool:** A publication answer may carry the completeness flag; it is rarely the reason an answer is thin.

### J17.1 — Three prefixes are mandatory

Every query needs the JOLux, SKOS and XSD prefixes.

**Figure:** Three prefixes.

**Consequence for a tool:** The prefix block belongs in one place, not in each query.

### J17.2 — OPTIONAL is mandatory

Not every field is present on every node; a query without OPTIONAL silently drops the incomplete rows.

**Figure:** 15 % of the acts have no status, 14.6 % no taxonomy entry.

**Consequence for a tool:** Every non-key field must be read as OPTIONAL.

### J17.3 — Subdivisions need the transitive step

The hierarchy is reached with `legalResourceSubdivisionIsPartOf+`; a single step answers only the direct children.

**Figure:** Transitive walk against a one-hop walk.

**Consequence for a tool:** A subdivision tool must state which walk it used.

### J17.4 — The language filter is a vocabulary IRI

Language is filtered by the EU publications-office IRI, not by a string.

**Figure:** Five values: de, fr, it, en, rm.

**Consequence for a tool:** The tool maps a friendly parameter onto the IRI and refuses anything else before querying.

### J17.5 — No published rate limit — be careful anyway

The endpoint publishes no rate limit. A complex query needs a timeout of at least thirty seconds, and a request must always identify itself with a user agent.

**Figure:** No official limit; 30 s named as the floor.

**Consequence for a tool:** The server sets an identifying agent, a bounded timeout per class and its own brake — politeness is a property of the code. This server DEVIATES from the thirty-second floor for one of the two classes, and says so here.

### J18.1 — Five interfaces between graph and text

Graph and text meet in five places: the manifestation URL, the eId comparison, impacts against footnotes, citations against in-text references, and the eId normalisation that joins them.

**Figure:** Five interface types.

**Consequence for a tool:** Each interface needs a tool on both sides, and each answer must name the side it came from.

### J18.2 — eIds must be normalised before they are compared

The graph writes `art_14a` where the manifestation writes `art_14_a`. Any comparison between the two sides must normalise first; verified for seven document types.

**Figure:** Six of seven types gain matches through normalisation.

**Consequence for a tool:** A read must accept both spellings, and the answer must say which element it actually opened.

### J18.2b — Annexes are components in the XML

What the graph calls an annex subdivision appears in the manifestation as a component, not as an attachment.

**Figure:** 1'611 annexes; 1'266 under a consolidated act.

**Consequence for a tool:** The annex tool must read components and hand out the path eIds the reader can open.

### J18.3 — Four layers make one life story

Consultation, draft, Official Compilation and classified compilation are one chain: political process, parliamentary process, binding publication, consolidated law.

**Figure:** Four layers, one act.

**Consequence for a tool:** The genesis tools must connect in that order, and each must point at the next.

### J19.1 — The consolidated URI is derivable

A consolidated act's URI follows from its basic act: `oc` becomes `cc`, a Federal-Gazette basis adds a suffix.

**Figure:** 99.5 % of the acts rest on an Official-Compilation basis.

**Consequence for a tool:** A tool may derive the counterpart URI, but must verify it rather than assume it.

### J19.2 — The consolidated text is not the legally binding one

The consolidated compilation is a consolidated presentation; the binding source is the Official Compilation.

**Figure:** Every consolidated act points at its basic act.

**Consequence for a tool:** A citation chain that claims bindingness must be able to reach the Official-Compilation act.

### J19.3 — Acts are grouped in weekly memorials

Official-Compilation acts belong to weekly issues under `eli/collection/oc/YYYY/NN`; the graph holds 238'107 of them from 1849 on.

**Figure:** 238'107 memorial instances.

**Consequence for a tool:** The memorial listing takes the issue ELI, not a page reference.

### J19.4 — The legislative life cycle is walkable

Draft → Federal Gazette → Official Compilation → classified compilation is one traversable chain, act by act.

**Figure:** The energy act's chain is recorded end to end.

**Consequence for a tool:** «How did this act come about» is answerable without leaving the graph.

### J19.5 — Three axes: work, expression, manifestation

Every resource exists as a language-agnostic work, a language-specific expression and a format-specific manifestation; the download URL hangs on the manifestation.

**Figure:** 3 languages × 4 formats = 12 manifestations for one version.

**Consequence for a tool:** The tool that lists formats must walk to the manifestation; the tool that reads must pick one.

### J20.1 — The legal-basis predicate is a phantom

`legalResourceLegalBasis` is defined in the ontology and filled nowhere.

**Figure:** 0 hits on both levels.

**Consequence for a tool:** No tool may promise the legal basis from the graph.

### J20.2 — Entry into force and applicability can diverge by years

The two dates are different facts and diverge by up to 927 days.

**Figure:** 927 days on SR 121.2.

**Consequence for a tool:** A precise temporal answer must say which date it used; the version's date is the applicability date.

### J20.3 — The taxonomy is the bridge between acts

85.4 % of the acts carry a hierarchical taxonomy entry; through the broader relation one reaches the whole field of law.

**Figure:** 59'218 of 69'350 acts; 9'027 entries.

**Consequence for a tool:** «Related acts» is answerable through the taxonomy, and the answer is a hint, never a norm.

### J20.4 — Consultation URIs follow one pattern

Every consultation URI reads `eli/dl/proj/YYYY/ID/cons_N`.

**Figure:** 100 % of a 50-sample.

**Consequence for a tool:** A consultation parameter can be validated by shape before any query.

### J20.5 — Consultation documents carry type codes

The documents of a consultation are typed by numeric codes in three super-categories: the draft itself, the explanatory material, the statements and the result report.

**Figure:** 3'143 drafts, 2'331 reports, 1'882 result reports.

**Consequence for a tool:** A document listing must carry the role, not only the URI.


## 3. Akoma Ntoso — the authentic text (X0–X20)

### X0.1 — Three collections, one markup

The analysed corpus holds 15'807 files: 3'409 consolidated acts, 3'204 Official-Compilation acts and 9'193 Federal-Gazette documents, together 5.7 million elements and 597'278 eIds.

**Figure:** 15'807 files, 323.5 MB, 597'278 eIds.

**Consequence for a tool:** The XML tools must be written against the consolidated collection and say so; the other two are a different shape.

### X0.2 — Every file parses

All 15'807 files are valid Akoma Ntoso 3.0 in the OASIS namespace; not one parse error.

**Figure:** 0 parse errors in 15'807 files.

**Consequence for a tool:** A parse failure is an upstream fault worth reporting as one, not a case to be handled routinely.

### X0.3 — The XML is the text, the graph is the metadata

The two sources are complementary: the graph knows 0 % of the text and 0.4–8.5 % of the structure, the XML knows 100 % of both.

**Figure:** 0 % text coverage in the graph.

**Consequence for a tool:** Every tool belongs to one side, and its answer names which.

### X0.3b — The analysed corpus is German only

All 15'806 files of the author's download carry `FRBRlanguage=de`; the French, Italian and Romansh manifestations were not part of it.

**Figure:** 100 % German in the analysed corpus.

**Consequence for a tool:** None — the figure describes a download, not the source.

### X1.1 — One document type

Every file is `<act name="publicLaw">`; `<doc>` occurs only inside a component. No bill, amendment or judgment appears at the top level.

**Figure:** 15'807 of 15'807 files.

**Consequence for a tool:** The reader needs no document-type dispatch — but it must not assume the same of a component.

### X1.2 — A third of the files have no body

5'378 files (34 %) carry only metadata, preface and sometimes a preamble — historical stubs, mostly old treaties. Only two of them carry components.

**Figure:** 5'378 of 15'807 files; 392 of them consolidated acts.

**Consequence for a tool:** «This version carries no text» must be an answer, not an exception.

### X2.2 — Seven metadata fields sit in every file

The ELI, the SR number, the German title, the language, the document date, the format and the author are present in every file's identification block.

**Figure:** 100 % of the files.

**Consequence for a tool:** A reader could take its metadata from the XML — this server takes it from the graph instead, which is a choice, not an oversight.

### X3.1 — The preface is universal

Every file carries a preface with the document number and title.

**Figure:** 100 % of the files; 16'910 document titles.

**Consequence for a tool:** An outline that starts at the body silently drops the title block.

### X3.2 — The preamble is universal for law, rare for the gazette

99.9 % of consolidated and Official-Compilation acts carry a preamble; only 28.1 % of Federal-Gazette documents do.

**Figure:** 99.9 % against 28.1 %.

**Consequence for a tool:** The preamble is part of the act and must be reachable by eId like any other element.

### X4.2 — Five document patterns, not one

The corpus divides into structured acts (2.8 %), flat article acts (23.5 %), level-based documents (34.9 %), body-less stubs (34.0 %) and pure amendment acts (3.4 %).

**Figure:** 435 / 3'712 / 5'514 / 5'376 / 544 files.

**Consequence for a tool:** An outline tool must keep the tree of a level-based document instead of assuming articles.

### X4.3 — The consolidated collection is mostly flat

74 % of consolidated acts are articles without chapters; only 9.1 % carry the full hierarchy.

**Figure:** 2'524 of 3'409 files.

**Consequence for a tool:** The outline must not promise chapters; the section path may be empty and that is normal.

### X5.1 — Paragraphs outnumber articles two to one

The corpus holds 170'565 paragraphs against 70'114 articles, 44'188 levels, 5'803 sections and 2'975 chapters.

**Figure:** 170'565 paragraphs.

**Consequence for a tool:** A paragraph is the ordinary address, not the exception — path eIds must be first-class.

### X5.2 — Most Official-Compilation and gazette files have no articles

84.4 % of Official-Compilation files and 91.1 % of Federal-Gazette files carry no article at all; consolidated acts carry a median of eleven.

**Figure:** Median 11 articles for consolidated acts, 0 elsewhere.

**Consequence for a tool:** Article-shaped reading works for the served collection only.

### X5.3 — The tree is five to twenty-four levels deep

Nesting runs from five to 24 levels, with a median band of ten to twelve for consolidated acts.

**Figure:** Max 24 in the Official Compilation.

**Consequence for a tool:** An outline must be capped by node count, and the cap must cut in document order rather than by depth.

### X6.1 — Six text elements carry the wording

`<p>` 1'481'735, `<item>` 233'363, `<content>` 186'697, `<blockList>` 72'969, `<listIntroduction>` 44'718, `<intro>` 1'060 — and the line break `<br/>` between them.

**Figure:** 1'481'735 paragraphs; 74 line breaks in the eight recorded manifestations, 21 of them with no whitespace on either side.

**Consequence for a tool:** The text writer must separate every element the corpus uses to break a line, or two printed lines run together in the answer and a quote of the printed wording cannot be verified.

### X6.3 — Tables are everywhere

21'836 tables with 216'831 rows; 73.8 % of consolidated files carry at least one, and the largest table has 5'308 rows.

**Figure:** 73.8 % of consolidated files; max 5'308 rows.

**Consequence for a tool:** A table must be answered as a unit with a visible cap, never as loose text.

### X6.4 — Footnotes carry the amendment history

77'349 authorial notes hold the change history as prose; 71.3 % carry a reference, the median note is 30 characters long.

**Figure:** 71.3 % with a reference; median 30 characters.

**Consequence for a tool:** The notes must be separated from the norm text and offered as their own answer.

### X7.2 — Every modification block holds exactly one quoted structure

All 12'481 modification elements carry exactly one quoted structure; in 95.6 % of cases the quoted root is a paragraph, not an article.

**Figure:** 12'481 of 12'481; 11'935 paragraphs.

**Consequence for a tool:** A consolidated act carries no modification blocks — the tool must say that rather than answer empty.

### X8.2 — Components are annexes to acts

Components are attachments inside files that do have a body; body-less files almost never carry them.

**Figure:** 5'195 components; 2 of 5'378 body-less files.

**Consequence for a tool:** The annex listing must look inside the act, not for a separate document.

### X9.2 — eIds follow ten prefixes

The address space is `art` (311'673), `annex` (80'645), `lvl` (52'352), `mod`, `list`, `sec`, `chap` and the reference anchors.

**Figure:** 311'673 article eIds.

**Consequence for a tool:** An address parser may rely on the prefixes but must refuse anything it does not know rather than guess.

### X9.3 — The eId notation is a path

An address reads `art_14`, `art_14a`, `art_14/para_1`, `art_14/para_1/lbl_a`, `chap_1/sec_2`, `annex_1`, `lvl_1`.

**Figure:** Seven documented shapes.

**Consequence for a tool:** The citation grammar must read and write exactly these — in both directions.

### X9.4 — Normalisation joins the two spellings

The XML writes `art_14_a` where the graph writes `art_14a`; a lookup must try both.

**Figure:** Verified for seven document types.

**Consequence for a tool:** The reader accepts either spelling and says which element it opened.

### X10.2 — An article is about 550 characters

The median article holds 551 characters; 62.6 % lie between 200 and 1'000, and 0.5 % exceed 5'000.

**Figure:** Median 551, mean 818, max 40'376 characters.

**Consequence for a tool:** A whole-document cap must be far above the article median, and a per-element answer needs no cap at all.

### X11.2 — Seven in ten references are absolute ELIs, fifteen per cent carry no target

Of 83'358 references 70.8 % point at an absolute Fedlex ELI, 15.0 % carry no href at all (the target is only in the text), 7.6 % point at other Fedlex resources and 6.5 % outside.

**Figure:** 58'511 ELI hrefs; 12'490 without href.

**Consequence for a tool:** The reference answer must carry the unlinked references too, and say how many there are.

### X12.2 — Eighty-one foreign tags occur

Beyond the standard the files carry SVG, MathML, Fedlex-proprietary and Office artefacts — placeholder (6'524) and block (5'005) most often.

**Figure:** 81 non-standard tags.

**Consequence for a tool:** Formulas and graphics must be reported as islands rather than flattened into the text.

### X15.1 — The XML quality is excellent

Not one parse error across the whole corpus; 147 distinct tags, consistently named.

**Figure:** 0 errors, 147 tags.

**Consequence for a tool:** A reader may treat a parse failure as an upstream incident.

### X15.3 — eId uniqueness is not guaranteed

842 of 15'806 files carry duplicate eIds — 9'304 duplicate entries in all, mostly in pre-1960 acts with complex annexes.

**Figure:** 842 files (5.3 %); 9'304 duplicates.

**Consequence for a tool:** A lookup must resolve to the first element in document order AND say that others carry the same address.

### X16.4 — Most tables are small, a few are enormous

The median table has one row and two columns; 75.3 % have five rows or fewer, 1.6 % have a hundred or more.

**Figure:** Max 5'308 rows, 36 columns.

**Consequence for a tool:** The row cap must be measured and reported, not silently applied.

### X17.1 — The files are not strictly schema-valid

Five schema violations exist, two of them major: 15 % of references carry no href, and the placement attribute of a footnote is never set.

**Figure:** 12'490 references; 77'190 footnotes.

**Consequence for a tool:** A reader must not validate against the schema and refuse — it reads what is there.

### X17.3 — Eleven hierarchy elements are in use

Of 28 possible hierarchy elements the corpus uses eleven — paragraph, article, level, section, chapter, title, subdivision, part, transitional, proviso and one book.

**Figure:** 32 transitional and 26 proviso elements.

**Consequence for a tool:** The citation grammar must have an answer for the rare ones too, or refuse them with their true reason.

### X17.6 — A file may carry several FRBR expressions

17.1 % of the files carry more than one `<FRBRExpression>` block — several language versions or fassungen in one file; 21'001 identification blocks in 15'806 files.

**Figure:** 13'111 files with one, 1'774 with two, 484 with three or more, up to 36.

**Consequence for a tool:** A tool must not read the document's language or version back from «the» expression block — there may be several, and the first is not authoritative.

### X17.7 — No conclusions, no attachments, no cover page

Those three container elements are never used; annexes are modelled exclusively as components.

**Figure:** 0 occurrences of each.

**Consequence for a tool:** The annex tool must read components, and nothing may look for an attachments block.

### X18.1 — Component documents look like acts

The 5'195 component documents use the same structures inside a main body; their median text is 2'212 characters and 267 are empty stubs.

**Figure:** Median 2'212 characters; 267 stubs under 100 characters.

**Consequence for a tool:** The annex listing must mark an empty component as a stub rather than answer an empty text.

### X18.2 — Four in five reference targets lie outside the corpus

Of 15'264 unique ELI URIs in `<ref>` elements only 3'061 (20.1 %) resolve inside the analysed corpus; the rest point at acts, gazette documents and treaties that are not in it.

**Figure:** 20.1 % resolvable locally.

**Consequence for a tool:** A reference is a HINT, never a promise that its target can be opened: link resolution against any local holding finds one target in five, so the target must be resolved through the graph, at the moment it is asked for.

### X18.3 — The text is clean but carries invisible characters

No encoding corruption; 3'131 soft hyphens and 662'695 no-break spaces are in the text, and 63 files carry substantial foreign-language passages.

**Figure:** 3'131 soft hyphens, 662'695 no-break spaces.

**Consequence for a tool:** Exact text matching must normalise them, or a verbatim quote fails for an invisible reason.

### X18.4 — MathML hides without a namespace

The formula elements inside foreign blocks carry no namespace of their own and must be recognised by their local names.

**Figure:** 460 MathML elements in 24 files.

**Consequence for a tool:** Island detection must look at names, not at namespaces.

### X18.5 — Inline markup is presentation

The 14'142 inline elements are formatting overrides from the Word conversion and carry no meaning.

**Figure:** 4'726 style and 4'709 weight overrides.

**Consequence for a tool:** Text extraction flattens them; nothing may branch on them.

### X18.7 — Signatures stand at the end of the body

1'289 signature elements sit directly in the body, with a median of 364 characters.

**Figure:** 1'289 elements, 100 % in the body.

**Consequence for a tool:** A whole-document read must not lose them; an outline may treat them as their own unit.

### X19.1 — The identification block is the FRBR chain

Work, expression and manifestation appear in every file, with the language on the expression and the format on the manifestation; the item level is never used.

**Figure:** 21'001 identification blocks in 15'806 files.

**Consequence for a tool:** The reader may trust the chain in the file — but this server resolves it in the graph, where the versions are.

### X19.3 — Author and date are mirrored, the country is always CH

Author and date appear identically on the work and the expression; the country is CH in every file.

**Figure:** 171'573 author elements; 100 % CH.

**Consequence for a tool:** Reading either level suffices — an answer must not present the mirror as two facts.

### X19.6 — References point at works, never at expressions

Of 58'511 ELI references 58'495 address the work; sixteen address a language expression and none a format.

**Figure:** 100.0 % work level.

**Consequence for a tool:** A reference answer must not promise a language — the target is the abstract act, and resolving it needs a version step.

### X19.8 — A component is its own work

All 5'195 component documents carry a complete identification of their own.

**Figure:** 5'195 of 5'195.

**Consequence for a tool:** The annex listing may name the component's own work URI — it is a real identifier, not a derived one.

### X19.9 — The dates run into the future

Expression dates reach from 1852 to 2204 — future dates are planned entries into force, not errors.

**Figure:** 1852–2204.

**Consequence for a tool:** A date far in the future is data, not a fault; a version list must keep it and a Stichtag after today must be marked as a projection.


## 4. Conformance table

One row per restated rule, measured against the tree of 2026-08-30.
Counts: **123 rules restated** — 99 honoured,
3 violated, 15 untested,
6 not applicable, 0 unknown.
The test column names the function that pins the rule; `—` means none
does. `tests/rules_table.rs` fails if an `honoured` row names a
function this crate does not carry.

| id | tool(s) | test (file::function) | status | note |
|---|---|---|---|---|
| `J0.1` | read_article, get_law_metadata | tests/e2e.rs::read_article_delivers_eid_precise_norm_text_from_the_recorded_manifestation | honoured | The server splits the families by construction (ENGINE.md §4: JOLux tools vs XML tools) and every XML answer carries `served`. |
| `J0.2` | all | — | untested | `FEDLEX_ENDPOINT` and `MANIFESTATION_HOST` are one host, and `fetch_manifestation` refuses a URL outside it — but no test in this crate exercises that refusal. The engine-conformance gate in `tools/check.sh` compares the declared egress against the sources, which is a gate, not a test of this behaviour. |
| `J0.3` | explore_node | tests/e2e.rs::explore_node_shows_both_directions_capped | honoured | `explore_node` says it is a debugging view and caps both directions. |
| `J1.1` | all | — | untested | The 35 tools do cover the CC core, impacts, subdivisions, citations, consultations, drafts and treaties — but «no tool addresses a class outside these nineteen» is an architectural claim no test makes. `stdio_session_serves_the_toolset` proves the surface answers, not its mapping. |
| `J1.2` | get_law_metadata, get_oc_act | tests/e2e.rs::the_official_compilation_chain_of_the_bgoe | honoured | `get_law_metadata` answers title, status, dates and identifier only; `get_oc_act` answers genre and responsible office — the recorded BGÖ chain shows both. |
| `J2.1` | read_article, get_structure, search_text, read_document, extract_tables, compare_versions, check_quote, cite | tests/e2e.rs::read_article_delivers_eid_precise_norm_text_from_the_recorded_manifestation | honoured | `load_version` writes `?cons jolux:isMemberOf <abstract>`; every XML tool rides that one query. |
| `J2.2` | resolve_consolidation_at, check_in_force | tests/e2e.rs::the_bitemporal_loop_resolves_governing_versions_honestly | honoured | The bitemporal loop resolves and stamps `valid_as_of`; the caller may request a date, only the server writes what applied. |
| `J2.3` | list_expressions, read_article | tests/e2e.rs::list_expressions_shows_pdf_only_before_a_read | honoured | `list_expressions` shows the formats before a read; `load_version` answers not-found with the PDF-only ground. |
| `J3.1` | get_law_metadata, check_in_force | tests/e2e.rs::check_in_force_reads_the_acts_own_end_of_force | honoured | Since the BV addendum both tools read the act's own `dateEntryInForce`, `dateNoLongerInForce` and `dateEndApplicability`; `dateApplicability` is read on the consolidation only, where it is the version's own date. The recorded Energy Act of 1998 proves the whole rule. |
| `J3.2` | check_in_force | tests/e2e.rs::check_in_force_reads_the_acts_own_end_of_force; src/domain.rs::the_in_force_rule_names_the_field_that_decided | honoured | FIXED in the BV addendum, finished at A′. `check_in_force` bridges the vendored JLX-TMP-03 query — entry into force and BOTH end dates, the earlier one deciding, the status vocabulary only where the act carries no date — and the governing consolidation stays beside the answer without deciding it. The A′ audit found the second half of the consequence missing: the answer showed two end dates and never said which one it used, and no recorded act carries `dateEndApplicability` at all, so the recorded case could not prove the rule either. The answer now carries `decided_by`, and the rule itself — both dates read, the EARLIER deciding, the status only where no date exists — is held by a unit test on the shared function, where no fixture is needed. Recorded live for the case: the Energy Act of 1998 (`cc/1999/27`), in force on 2017-06-01, ended on 2018-01-01. |
| `J3.3` | search_law, resolve_sr, get_law_metadata, list_versions, check_in_force | tests/e2e.rs::search_law_keeps_the_acts_that_carry_no_in_force_status; tests/e2e.rs::an_act_without_consolidations_answers_from_its_profile | honoured | The recorded «StPO» window carries status-less acts (JStPO among them) and they survive the OPTIONAL. The BV addendum went further: an act the graph knows and never consolidated (`cc/2020/2930_cc`, recorded) answers an EMPTY version list with its reason and `check_in_force` answers `status_unset: true, no_enforcement_data: true` — only an ELI the graph knows nothing about is a not-found. |
| `J3.4` | search_law, cite, parse_reference | tests/e2e.rs::cite_names_the_canonical_fundstelle_in_the_language_read | untested | DOWNGRADED at A′. The fallback exists in the code — `cite` writes «(SR …)» where the graph carries no abbreviation for the language read — but no recorded act takes that branch: all three recorded cite profiles carry a `titleShort` in every language whose manifestation exists, and the note's own example was wrong (the Italian label asserted in the test is «art. 6 cpv. 1 LTras», an abbreviation, not «(SR 152.3)»). What would prove it: one recorded act without a `titleShort` in the language read — an international text is the likely candidate — and the same for `search_law`'s abbreviation pre-query falling back to the title search. |
| `J3.5` | search_law, find_treaties | tests/e2e.rs::search_law_finds_an_act_by_its_multi_word_official_title; tests/e2e.rs::search_law_ranks_the_act_above_the_treaty_that_shares_its_words; tests/e2e.rs::the_promulgation_date_census_of_the_recorded_titles; src/domain.rs::the_title_filter_asks_for_every_word_and_for_one_word_is_unchanged | violated | Half answered at BY point 0. `search_law` asks for every WORD of the query in the same title (`all_words_in`), so «Bundesgesetz über die politischen Rechte» answers the BPR (`cc/1978/688_688_688`, SR 161.1) first instead of the UNO covenant; for a one-word query the fragment is byte-for-byte the contiguous filter it replaced, and for more words the match set can only grow. The unit test pins the filter's SHAPE, because the fixture backend answers by semantic key and never reads the SPARQL, and the census test pins the figure. `find_treaties` still builds ONE contiguous `CONTAINS(LCASE(STR(?title)), …)` over `jolux:titleTreaty`, so a treaty asked for by its official title is missed the same way an act was. Ranked fix: (1) `find_treaties` reads through `all_words_in`, with the same twelve-word refusal; (2) one recorded window for a multi-word treaty title, one polite request. |
| `J4.1` | get_subdivisions, get_structure | tests/e2e.rs::get_subdivisions_is_a_gap_catalogue_with_eids | honoured | `get_subdivisions` carries the note «gap catalogue: JOLux knows only elements with at least one amendment — the outline is fedlex.get_structure». |
| `J4.2` | get_subdivisions | tests/e2e.rs::get_subdivisions_is_a_gap_catalogue_with_eids | honoured | The recorded BGÖ answers 12 subdivisions against an outline of far more elements; the note names the reason. |
| `J4.3` | read_article, get_structure | tests/e2e.rs::get_structure_outlines_the_act_with_eids_and_headings | honoured | The outline is read from the manifestation, and path eIds travel from it into `read_article`. |
| `J4.4` | get_subdivisions | tests/e2e.rs::get_subdivisions_is_a_gap_catalogue_with_eids | honoured | Every row carries `type` as the vocabulary IRI, decodable through `resolve_vocabulary_label`. |
| `J4.5b` | get_subdivisions | tests/e2e.rs::get_subdivisions_is_a_gap_catalogue_with_eids | honoured | The answer carries `cap`, `truncated` and `truncation_basis` («cap reached (the vendored primitive's LIMIT 500) — not a count»), all three asserted. A′ corrected the second half of this note: the walk was named only in a Rust doc comment, which no answer and no test can carry — it is now the `walk` field of the answer (J17.3). |
| `J5.1` | resolve_vocabulary_label | tests/e2e.rs::resolve_vocabulary_label_works_by_label_by_iri_and_locally_for_languages | honoured |  |
| `J5.3` | resolve_vocabulary_label, get_taxonomy, get_law_metadata, resolve_sr, check_in_force, parse_reference | tests/e2e.rs::the_status_vocabulary_is_decoded_in_every_act_answer; src/domain.rs::a_label_is_chosen_in_the_language_the_graph_actually_has | honoured | Both directions are served by `resolve_vocabulary_label` (label → IRIs inside a scheme, IRI → labels in every language). Since the BV addendum the ACT answers decode the status themselves: `get_law_metadata`, `resolve_sr` (its `also_matches` rows included) and `check_in_force` join `skos:prefLabel` and derive `in_force` from the one rule. A′ closed the two remaining de-only spots: the vendored `check_in_force` reads the label in German only, so where the catalogue carries none the server fetches the status IRI's labels once and the fallback de → en → fr → it → rm decides — `status_label_lang` says which language answered; and `parse_reference`'s SR branch now repeats the profile's decision (`in_force`, `status_label`, `status_unset`) instead of comparing the status IRI itself. Partner countries of a treaty stay IRIs and the note names the decoder. |
| `J5.4` | resolve_vocabulary_label | tests/e2e.rs::resolve_vocabulary_label_works_by_label_by_iri_and_locally_for_languages; src/domain.rs::a_label_is_chosen_in_the_language_the_graph_actually_has | honoured | Fixed at BV A′, and the gap was wider than the six catalogues. The IRI branch asked for ONE language and answered nothing where the concept had no label in it; the label SEARCH returned matches with `label: null` — measured: ALL TWELVE matches of «Code» in `legal-subject-theme-fr` carry no German label. Both branches now read every `skos:prefLabel` of the concept (one query, `vocabulary_labels:<iri>`; for a search one query over exactly the concepts that lack a label) and choose de → en → fr → it → rm. The IRI answer names `answered_in`, every search match carries `label_lang`, and `labels_filled` counts what had to be filled: `legal-subject-theme-fr/22158` answers «Code» in French to a German request. |
| `J5.5` | get_law_metadata, search_law | tests/e2e.rs::metadata_carries_profile_and_provenance | untested | `get_law_metadata` carries `identifier` and status but no `typeDocument`; the type is reachable through `explore_node` only. Not wrong, but nothing pins it — a field to consider in the next contract wave. |
| `J6.1` | get_article_history | tests/e2e.rs::get_article_history_matches_the_element_exactly_and_joins_consolidations | honoured | The answer carries impact URI, type, type label, date, source act and the consolidation. |
| `J6.2` | get_article_history | tests/e2e.rs::get_article_history_matches_the_element_exactly_and_joins_consolidations | honoured | The answer carries `completeness_note` naming exactly this. |
| `J6.3` | get_article_history | tests/e2e.rs::get_article_history_matches_the_element_exactly_and_joins_consolidations | violated | Downgraded at BV A′ after re-reading the rule against the query. The main query DOES read `impactToLegalResourceComment` — but only for impacts whose target is the article or a descendant of it. A comment-only impact hangs at the ACT level and names the article inside its free text, so this tool never sees it: the free-text half is exactly what is lost. A live probe on 2026-08-29 suggested the BGÖ's act-level comments are empty strings, but that probe was never recorded as a fixture and is therefore NOT evidence this tree carries (A″: an unrecorded measurement is a memory, not a record) — it is struck from this row, and the rulebook's own 38'701 comment-only impacts stand. Ranked fix: (1) a recorded survey of impacts with a non-empty comment — where do they hang, how is the article written inside them, one polite request per key; (2) one extra query per call, filtered server-side on the article's number and refined by a word-boundary match, answered as a DECLARED hint (`comment_matches`) beside `impacts`, never merged into them; (3) `completeness_note` then names the scan instead of the gap. Until then the note says the answer may be incomplete and points at `get_modifications`. |
| `J6.4` | get_article_history | tests/e2e.rs::get_article_history_matches_the_element_exactly_and_joins_consolidations | honoured | `completeness_note` names the 2023 system change and points at `get_modifications` for the notes the authentic text carries. |
| `J6.5` | get_article_history | — | untested | The tool queries the graph and never touches the manifestation, so the case would answer; no recorded fixture carries a repealed article to prove it. |
| `J7.1` | get_citations | tests/e2e.rs::get_citations_serves_the_formal_citation_graph_as_directions | honoured | The coverage string says «formal citations at act level» and the in-text references are pointed at `get_references`. |
| `J7.2` | get_citations | tests/e2e.rs::get_citations_serves_the_formal_citation_graph_as_directions | honoured | The figure has moved: the recording of 2026-08-29 carries the description on the BGÖ's outgoing citations («Art. 20 Auskunfts- und Einsichtsrechte» — the citing element's heading). The tool passes it through and the test pins it, so a later emptying is noticed. |
| `J7.3` | get_citations, get_references | tests/e2e.rs::get_citations_serves_the_formal_citation_graph_as_directions; tests/e2e.rs::get_references_lists_linked_and_unlinked_refs_as_hints_with_scope | honoured | The A′ audit found the reciprocity half-built: `get_citations` named `get_references` in its coverage, `get_references` named nothing. It now carries the pointer AND the 0–48 % overlap figure in its own coverage string, so a reader of either answer is told the other half exists — and both are asserted. |
| `J7.4` | get_citations | tests/e2e.rs::get_citations_serves_the_formal_citation_graph_as_directions | honoured | The vendored primitive deduplicates client-side (its query has no DISTINCT, by WAF necessity); the test asserts 17 distinct targets from the recorded 242 rows. |
| `J8.1` | get_oc_act, get_memorial | tests/e2e.rs::the_official_compilation_chain_of_the_bgoe | honoured |  |
| `J8.2` | get_oc_act, read_article | tests/e2e.rs::the_official_compilation_chain_of_the_bgoe | honoured | `get_oc_act` answers metadata only and never offers to read an Official-Compilation text; the XML tools take a consolidation version. |
| `J8.3` | get_oc_act | tests/e2e.rs::the_official_compilation_chain_of_the_bgoe | honoured |  |
| `J8.5` | get_memorial | tests/e2e.rs::the_official_compilation_chain_of_the_bgoe | honoured | `get_memorial` caps at 50 with cap+1 truncation; the recorded issue answers 15 acts. |
| `J9.1` | get_fga_documents | tests/e2e.rs::the_genesis_of_the_ndsg_is_reachable_drafts_consultations_documents | honoured | The tool answers documents with genre and label and is a `norm` about the publication, not about the law. |
| `J9.3` | get_fga_documents, list_versions, resolve_consolidation_at, get_article_history, get_citations, get_subdivisions, check_in_force | tests/e2e.rs::the_version_history_and_citation_tools_refuse_another_collection | honoured | Only `get_fga_documents` touches the collection, and it answers documents with genre, label and publication date. The A′ audit found the other half unbuilt: nothing STOPPED a caller from handing a gazette IRI to the version, history or citation tools, which would have queried and answered an empty list — an invention dressed as an answer. The six act-scoped tools now refuse a document of another collection (`/eli/fga/`, `/eli/oc/`, `/eli/collection/`, `/eli/dl/proj/`) before any request, and the refusal names the tool that does answer for it. |
| `J9.4` | search_law | tests/e2e.rs::search_is_a_hint_never_a_norm | honoured | `search_law` queries `ConsolidationAbstract` only, so the classified compilation is the served scope by construction; the answer says so when it is empty. |
| `J10.1` | get_consultations | tests/e2e.rs::the_dsg_revision_consultation_is_reachable_with_dates_and_documents | honoured |  |
| `J10.2` | get_consultations | tests/e2e.rs::the_dsg_revision_consultation_is_reachable_with_dates_and_documents | honoured | The query walks `hasSubTask` for start date, end date and institution — the recorded DSG-revision consultation answers 2016-12-21 → 2017-04-04 from the opening phase. |
| `J10.3` | get_consultations | tests/e2e.rs::the_dsg_revision_consultation_is_reachable_with_dates_and_documents | untested | The tool reads level one only. The recorded consultation carries both; adding the second level is a small contract change, not a defect. |
| `J10.4` | get_consultations | tests/e2e.rs::the_dsg_revision_consultation_is_reachable_with_dates_and_documents | honoured | `get_consultations` answers `kind: hint`; the stage-one line says «never for law in force». |
| `J11.1` | get_drafts | tests/e2e.rs::the_genesis_of_the_ndsg_is_reachable_drafts_consultations_documents | honoured |  |
| `J11.2` | get_drafts | tests/e2e.rs::the_genesis_of_the_ndsg_is_reachable_drafts_consultations_documents | honoured | Every draft row carries `parliament_draft_id` and the note names Curia Vista. |
| `J11.3` | get_drafts | tests/e2e.rs::every_stage_one_line_follows_the_house_rule | honoured | The stage-one line reads «use as the entry to consultations and materials». |
| `J12.1` | find_treaties | tests/e2e.rs::treaties_are_found_by_a_title_word_and_profiled | honoured | `find_treaties` caps at 50 with cap+1 truncation; the recorded «Menschenrechte» window answers ten. |
| `J12.2` | find_treaties | tests/e2e.rs::treaties_are_found_by_a_title_word_and_profiled | untested | The filter exists (`bilateral: true\|false`); no recorded fixture exercises it, and the unknown third state is not addressable. |
| `J12.4` | resolve_sr, read_article, cite | tests/e2e.rs::the_emrk_chain_is_recorded_reality_treaty_manifestations_included | honoured | The recorded chain resolves 0.101, lists versions and reads Art. 3 as a norm. |
| `J13.1` | read_article, list_expressions | tests/e2e.rs::read_article_reads_the_english_manifestation_and_names_its_language; tests/e2e.rs::list_expressions_shows_pdf_only_before_a_read | honoured | Fixed at BV A′. Before it, `manifestation_lang` refused «en» and «rm» before asking the graph — the tool deciding what the data says. It now accepts all five official languages: the recorded BGÖ 2023-11-01 carries XML in de, fr, it, rm AND en, and `read_article(…, «en»)` reads it. A version without XML in the language asked answers not-found with the PDF-only ground (the KVG's 1996 consolidation); a language the vocabulary does not define («es») is refused before any request. The same fix closed an absent finding: every XML answer now names the version and the language it read — `read_article` was the one that did not. |
| `J13.2` | read_article, check_quote, cite | tests/e2e.rs::the_same_eid_reads_in_every_language_of_the_version | honoured | Test added at BV: `art_6` of the BGÖ reads in de, fr and it with the same eId and three different texts. |
| `J13.3` | read_document | tests/e2e.rs::read_document_is_capped_with_the_original_length_and_a_continuation | honoured | The cap is applied to the served text and the answer carries `total_chars` and a continuation offset. |
| `J14.1` | read_article, list_expressions | tests/e2e.rs::xml_tools_refuse_bad_versions_before_any_fetch | honoured | `load_version` answers not-found naming the upstream reality and pointing at `list_expressions`. |
| `J14.1b` | list_versions | tests/e2e.rs::the_bitemporal_loop_resolves_governing_versions_honestly | honoured | `list_versions` answers every consolidation with its date, future ones included, ordered. |
| `J14.2` | resolve_consolidation_at, read_article, compare_versions | tests/e2e.rs::compare_versions_finds_the_changed_and_inserted_articles | honoured | The BGÖ diff reads two 2023 consolidations; the older-version branch answers not-found with the ground where XML is absent. |
| `J14.3` | resolve_consolidation_at | tests/e2e.rs::the_bitemporal_loop_resolves_governing_versions_honestly | honoured |  |
| `J15.1` | search_law | tests/e2e.rs::search_law_ranks_the_act_in_force_first | honoured | The BO′ ranking sorts in-force first, then by the systematic number's own order — the law before its ordinances, never by document type. |
| `J15.2` | list_annexes, extract_tables, cite | tests/e2e.rs::list_annexes_names_path_eids_that_read_article_reads | honoured |  |
| `J15.3` | get_law_metadata, check_in_force | tests/e2e.rs::the_bitemporal_loop_resolves_governing_versions_honestly | untested | Dates are parsed as ISO and never bounded by a range; no recorded act predates 1900, so nothing pins the old end. |
| `J16.1` | get_citations, get_law_metadata | tests/e2e.rs::the_impact_directions_say_what_they_are | honoured | FIXED in the BV addendum: the stage-one line no longer promises «who amends X» (it promised what the field cannot deliver — the recorded KVG answer at the incoming end is 33 consultation drafts), and the coverage now names the field, its 0.8 % fill rate, the absence of type and date, and the two tools that answer the questions this one does not. |
| `J16.2` | — | — | not_applicable | No tool references them; nothing to test. |
| `J16.3` | get_oc_act | — | untested | The tool does not read the completeness predicate; the vendored primitive does not offer it. |
| `J17.1` | all | — | untested | `PREFIXES` is one constant and the XSD prefix is added where a typed date literal is built; no test asserts that a query carries them. A query missing a prefix would fail upstream, which is how it would be noticed — after the fact. |
| `J17.2` | search_law, resolve_sr, get_law_metadata, get_taxonomy | tests/e2e.rs::search_law_keeps_the_acts_that_carry_no_in_force_status | honoured | Proven for the status; the same discipline is written into every hand-built query. |
| `J17.3` | get_subdivisions | tests/e2e.rs::get_subdivisions_is_a_gap_catalogue_with_eids | honoured | The vendored primitive walks transitively — and since A′ the ANSWER says so: a doc comment is not a statement the caller can read. The `walk` field names the transitive shape and why the number is not comparable to a direct-children count. |
| `J17.4` | read_article, list_expressions, resolve_vocabulary_label | src/domain.rs::manifestation_lang_maps_the_five_onto_their_vocabulary_iris; tests/e2e.rs::xml_tools_refuse_bad_versions_before_any_fetch | honoured | `manifestation_lang` maps all five official languages onto their EU publications-office IRIs and refuses anything else as invalid-input, before a request leaves the process. A′ added the unit test that pins the five pairs — the e2e test showed the refusal, not the mapping. |
| `J17.5` | all | src/backend.rs::the_timeout_constants_bound_every_live_request_against_the_caller | honoured | Fixed at BV, restated honestly at A′. The rulebook names 30 s as the floor for a complex query; this server sets 15 s for a SPARQL select and 30 s for a manifestation fetch. The deviation is deliberate and is the server's own: the queries are fixed templates, not exploratory ones, and the CALLER's budget is 15 s — the brake may reserve 5 of them, so a select that outlives it answers nobody. A select that needs longer than 15 s is a query this server should not be sending. Both call paths AND both body-read paths name the class and the bound in their refusal (A′ closed the two body reads, which said only «body»). What the named test proves is the CONSTANTS — the agent string and the two bounds every live path hands to ureq — not a stalled connection; the test's name says so since A′. |
| `J18.1` | read_article, get_article_history, get_modifications, get_citations, get_references | tests/e2e.rs::get_modifications_anchors_change_notes_at_their_elements | honoured | All five have a tool pair; every XML answer carries `served` and every graph answer its `source`. |
| `J18.2` | read_article, get_article_history, cite | tests/e2e.rs::read_article_accepts_the_jolux_spelling_and_says_how_it_resolved | honoured | Test added at BV: `art_25a` opens `art_25_a` and the answer carries `eid_via_normalisation: true`. |
| `J18.2b` | list_annexes | tests/e2e.rs::list_annexes_names_path_eids_that_read_article_reads | honoured |  |
| `J18.3` | get_consultations, get_drafts, get_oc_act, get_memorial | tests/e2e.rs::the_genesis_of_the_ndsg_is_reachable_drafts_consultations_documents | honoured | The recorded nDSG chain walks act → draft → consultation → documents, and the AS chain act → oc → memorial. |
| `J19.1` | get_oc_act | tests/e2e.rs::the_official_compilation_chain_of_the_bgoe | honoured | `get_oc_act` asks the graph for `basicAct` instead of rewriting the string, and refuses an Official-Compilation ELI as input with a pointer. |
| `J19.2` | get_oc_act, cite | tests/e2e.rs::the_official_compilation_chain_of_the_bgoe | honoured | `get_oc_act` reaches the binding publication from any consolidated ELI; the recorded BGÖ answers `eli/oc/2006/355`. |
| `J19.3` | get_memorial | tests/e2e.rs::the_official_compilation_chain_of_the_bgoe | honoured | `get_memorial` takes an oc ELI and refuses a consolidated one with a pointer; the recorded issue is `eli/collection/oc/2006/24`. |
| `J19.4` | get_drafts, get_fga_documents, get_oc_act | tests/e2e.rs::the_genesis_of_the_ndsg_is_reachable_drafts_consultations_documents | honoured |  |
| `J19.5` | list_expressions | tests/e2e.rs::list_expressions_shows_pdf_only_before_a_read | honoured |  |
| `J20.1` | — | — | not_applicable | No tool references it. |
| `J20.2` | check_in_force, resolve_consolidation_at | tests/e2e.rs::the_bitemporal_loop_resolves_governing_versions_honestly | honoured | The version answer carries `valid_as_of` (the applicability date of the consolidation) and the act profile carries `entry_in_force` separately. |
| `J20.3` | get_taxonomy, find_related_topic | tests/e2e.rs::find_related_topic_returns_capped_hints_by_eli_or_sr | honoured | `get_taxonomy` answers notation, labels and the branch chain; `find_related_topic` answers capped hints. |
| `J20.4` | get_consultations, get_consultation_documents | tests/e2e.rs::the_dsg_revision_consultation_is_reachable_with_dates_and_documents | honoured | Both tools validate the IRI shape before querying and refuse anything else as invalid-input. |
| `J20.5` | get_consultation_documents | tests/e2e.rs::the_dsg_revision_consultation_is_reachable_with_dates_and_documents | honoured | Since BS every document carries `role` (draft \| related \| opinion), its class and its German title — the recorded consultation answers fourteen. |
| `X0.1` | read_article, get_structure, search_text, read_document | tests/e2e.rs::the_kvg_outline_is_large_and_the_lsv_carries_annexes | honoured | Every XML tool takes a dated consolidation; the Official Compilation and the Federal Gazette are answered as metadata only. |
| `X0.2` | read_article, get_structure | tests/e2e.rs::a_manifestation_that_does_not_parse_is_an_upstream_fault | honoured | A manifestation that does not parse answers `upstream-unavailable` with the parser's message — never a silent empty. The A′ audit found the branch untested and it could not be otherwise: every recorded fixture is a real, well-formed manifestation. The test now BUILDS the case — the recorded resolution answer beside a body that is not Akoma Ntoso, in a temporary fixture directory — so the branch is exercised without a fabricated recording entering the fixture set. |
| `X0.3` | all | tests/e2e.rs::fixtures_are_served_as_fixtures_never_as_live_or_cache | honoured | Every XML answer carries `provenance.served` (live \| cache \| fixture) and the test pins all three states; the graph answers carry `source` instead. A′ decided one absent finding under this rule and records it as a CONTRACT point, not a data rule: an element answer carries NO in-force status. Wording and force are the two sides — `read_article` answers what a dated version says, `check_in_force` answers whether the act is in force at a date — and a status field on the text answer would invite «this is current law» from an answer that only knows a wording. The version date stands in `provenance.valid_as_of`, and the in-force question has its own tool. |
| `X0.3b` | — | — | not_applicable | An artefact of the analysed corpus, not of Fedlex: this server fetches the language it is asked for, and reads all five official languages the graph offers (J13.1). |
| `X1.1` | read_article, list_annexes | — | untested | Components are read through the vendored component reader, which expects the `<doc>` shape; nothing asserts that a top-level document type other than `act` is impossible — the recorded manifestations are all acts. |
| `X1.2` | read_document, get_structure | — | untested | No recorded manifestation is body-less; the code would answer an empty outline and an empty document rather than an error, but nothing pins it. A recording would settle it — named in the report as a missing fixture. |
| `X2.2` | get_law_metadata | — | not_applicable | The server answers metadata from the graph (one query, every language) and never parses the XML identification block. |
| `X3.1` | get_structure | tests/e2e.rs::get_structure_outlines_the_act_with_eids_and_headings | untested | The recorded outlines begin at the body's sections; whether a preface element would appear is not pinned. |
| `X3.2` | read_article, get_structure | tests/e2e.rs::read_article_accepts_path_eids_and_refuses_malformed_ones | untested | `read_article` reads any eId-bearing element, the preamble included; no recorded test addresses one. |
| `X4.2` | get_structure | src/domain.rs::depth_article_cuts_below_articles_only | honoured | The unit test pins exactly this: depth «article» cuts below articles, and a level-based document without articles keeps its tree. |
| `X4.3` | get_structure, read_article | tests/e2e.rs::get_structure_outlines_the_act_with_eids_and_headings | honoured | The recorded BGÖ outline starts at `sec_1`, the KVG's is deep; `read_article` carries `section_path` and it may be empty. |
| `X5.1` | read_article, cite, check_quote | tests/e2e.rs::read_article_accepts_path_eids_and_refuses_malformed_ones | honoured | Path eIds travel from the finding tools into the reading tools; `cite` labels them as «Abs. n». |
| `X5.2` | read_article | — | not_applicable | Collection not served: the XML tools take a consolidation version. |
| `X5.3` | get_structure | src/domain.rs::outline_cap_is_a_document_order_prefix | honoured | The unit test pins that the cap is a document-order prefix; the answer carries `truncated` and the original size. |
| `X6.1` | read_article, read_document, search_text, check_quote, extract_tables, cite | tests/e2e.rs::a_line_break_runs_two_lines_together_in_the_answer; tests/e2e.rs::the_line_break_census_of_the_recorded_manifestations | violated | Unfolded at BV A′ (it was folded into X5.1 as a census without a consequence — it has one). The vendored text writer separates `num\|td\|th` with a space and `p\|paragraph\|heading\|item\|listIntroduction\|intro\|tr\|block\|content` with a newline; `<br/>` is in no list, so it produces NOTHING. Measured by the SUITE, not by hand (A″): the census test walks the eight recorded manifestations and counts 74 line breaks — 53 with whitespace immediately before the tag or after it, which read correctly BY ACCIDENT, and 21 with none, which run two words together. An earlier hand count said 66/51/15 with an undocumented window; the test's definition is the figure now. The LSV's limit-value table answers «PlanungswertLr in dB(A)» and «ImmissionsgrenzwertLr in dB(A)», and `check_quote` of the printed «Planungswert Lr in dB(A)» is `verified: false` while the run-together form is true. Ranked fix: (1) our own text writer mirroring the vendored one plus `br`, applied at the ONE place every tool reads through, so no two tools disagree about the same element's text; (2) alternatively a marked patch of the vendored `dom.rs` with a PROVENANCE.md entry — the house rule keeps `third_party/` read-only, so (1) is the way. Not fixed here because it changes what every XML answer contains, which is its own wave. |
| `X6.3` | extract_tables | tests/e2e.rs::extract_tables_returns_the_lsv_limit_values_with_a_recognisable_header; src/domain.rs::a_second_header_row_is_kept_as_a_data_row_and_named | honoured | Caps 50 tables × 200 rows with the original sizes; the recorded LSV answers twelve tables. A′ decided the second-header case the vendored extractor dropped: it keeps `<th>` cells only as the header and only in row 0, so a row of `<th>` cells further down — the sub-header of a column group — fell out with everything in it. The rows are walked here now; such a row is kept as a DATA row (its cells carry the column meaning the numbers below need, and dropping data is the one thing a table tool may not do) and its index is named in `sub_header_rows`, so a reader can tell it from a measurement. Fedlex marks no `<th>` in the recorded corpus, so the case is proven on a synthetic table. |
| `X6.4` | read_article, get_modifications | tests/e2e.rs::get_modifications_anchors_change_notes_at_their_elements | honoured | `read_article` counts the notes and excludes them from the text; `get_modifications` answers them anchored at their elements, with the 71.3 % figure in its coverage string. |
| `X7.2` | get_modifications | tests/e2e.rs::get_modifications_anchors_change_notes_at_their_elements | honoured | The recorded consolidation answers `mod_blocks_total: 0` and the coverage says «mod blocks exist on amending acts only». |
| `X8.2` | list_annexes | tests/e2e.rs::list_annexes_names_path_eids_that_read_article_reads | honoured |  |
| `X9.2` | read_article, cite, parse_reference | src/domain.rs::eid_gate_accepts_paths_and_refuses_escapes | honoured | The eId gate accepts the known shapes and refuses escapes; `cite` names the shape it cannot label. |
| `X9.3` | cite, parse_reference | src/domain.rs::eid_components_invert_the_candidate_grammar | honoured | The unit test round-trips every shape between the written citation and the eId path. |
| `X9.4` | read_article | tests/e2e.rs::read_article_accepts_the_jolux_spelling_and_says_how_it_resolved | honoured | Test added at BV; the answer carries `eid_via_normalisation`. |
| `X10.2` | read_article, read_document | tests/e2e.rs::read_document_is_capped_with_the_original_length_and_a_continuation | honoured | The document cap is 120'000 characters by default with a continuation offset; an element answer is uncapped. |
| `X11.2` | get_references | tests/e2e.rs::get_references_lists_linked_and_unlinked_refs_as_hints_with_scope; src/domain.rs::a_reference_without_a_target_is_kept_and_counted | honoured | The coverage string carries the 70.8 % / 15 % figures, and since A′ the answer COUNTS the unlinked ones in `unlinked` instead of only promising to keep them. The audit's measurement is part of the record: no recorded manifestation carries a single href-less `<ref>` — all 54 of the BGÖ's are linked — so the recorded answer's honest value is 0 and the case itself is proven on a synthetic document, where a reference without a target survives into the answer with a null href and its label. |
| `X12.2` | detect_foreign_content | src/domain.rs::foreign_language_sections_and_islands_are_detected | honoured | The unit test proves a MathML island and a foreign-language section on a synthetic document. |
| `X15.1` | read_article | — | untested | A manifestation that does not parse answers `upstream-unavailable` with the parser's message (`load_version`); no test feeds a broken document — a synthetic one would pin it cheaply and is named in the report as the next cheap test. |
| `X15.3` | read_article, check_quote, cite | src/domain.rs::eid_resolution_reports_duplicates_and_normalisation; tests/e2e.rs::the_citation_pair_reports_the_eid_resolution | honoured | Fixed at BV: `read_article` carries `eid_duplicates`, and the addendum extended it to the citation pair — `check_quote` and `cite` report the resolution of the element they actually read (an annex wrapper reports its first level), and both notes say what a duplicate means for the answer. A synthetic document with a twice-used address proves the mechanism; no recorded manifestation carries one. |
| `X16.4` | extract_tables | tests/e2e.rs::extract_tables_returns_the_lsv_limit_values_with_a_recognisable_header; src/domain.rs::the_table_row_cap_is_measured_and_reported | honoured | The answer carries `rows_total` and `truncated` per table. The A′ audit found the cap never seen to BITE — no recorded table has more than 200 rows, so `truncated` was only ever asserted false — and a synthetic table of 250 rows now holds the other side: 250 reported, 200 returned, `truncated: true`, `oversized: true`. |
| `X17.1` | read_article, get_references | tests/e2e.rs::get_references_lists_linked_and_unlinked_refs_as_hints_with_scope | honoured | Nothing in the server validates against the schema; a reference without an href is answered as what it is. A′ corrected this note: the href-less case does not occur in ANY recorded manifestation (the audit checked all eight), so the claim rests on the synthetic proof under X11.2 and on the absence of a validator, not on the recorded corpus. |
| `X17.3` | cite, read_article | tests/e2e.rs::cite_labels_a_transitional_provision_by_its_heading | honoured | A transitional provision is cited by its heading; a structural element is refused as one and any other shape as «no citation grammar yet». |
| `X17.6` | read_article, read_document, detect_foreign_content, cite | tests/e2e.rs::read_article_reads_the_english_manifestation_and_names_its_language | honoured | Unfolded at BV A′ (it was folded into X19.1 as a count — it has a consequence of its own). This server never reads the language back from the file: `load_version` ASKS the graph for the manifestation in a language and carries that language through the answer, and `detect_foreign_content` compares the sections against the language that was asked for. The English read proves it end to end — the answer's `lang` is «en» because that manifestation was requested, not because a block in the file said so. |
| `X17.7` | list_annexes | tests/e2e.rs::list_annexes_names_path_eids_that_read_article_reads | honoured |  |
| `X18.1` | list_annexes | tests/e2e.rs::list_annexes_names_path_eids_that_read_article_reads | honoured | Every annex row carries a stub flag. A′ corrected this note: the recorded LSV annexes prove ONE state — all nine come back `is_empty_stub: false` — so what the test holds is the flag and the non-stub side; an empty component is not in the recorded corpus. |
| `X18.2` | get_references, parse_reference | tests/e2e.rs::get_references_lists_linked_and_unlinked_refs_as_hints_with_scope | honoured | Unfolded at BV A′ (it was folded into X19.7 as a figure — it has a consequence). This server holds no corpus, so it never resolves a reference locally: `get_references` answers `kind: hint` with the href as the corpus writes it (and an empty href where the corpus writes none), and the reader follows it with the graph tools. What the rulebook measured as a 20 % hit rate is the very reason the answer promises nothing. |
| `X18.3` | check_quote | src/domain.rs::quote_normalisation_folds_every_named_mark | honoured | The quote check folds whitespace, no-break spaces, soft hyphens, dashes and typographic quotation marks on both sides; a recorded LSV row proves the no-break space case. |
| `X18.4` | detect_foreign_content | src/domain.rs::foreign_language_sections_and_islands_are_detected | honoured |  |
| `X18.5` | read_article, check_quote | tests/e2e.rs::check_quote_answers_the_quote_table_verbatim_and_normalised | honoured | A′ corrected this note: the KVG paragraph it named carries typographic quotation marks and a no-break space — that row proves X18.3, not this rule. The quote table now carries a row whose recorded wording runs THROUGH an inline override (the LSV writes «37a» with the letter in an `<i>` element), and the check verifies it, which is what «text extraction flattens them» has to mean. |
| `X18.7` | read_document, get_structure, search_text | tests/e2e.rs::body_level_text_is_rendered_named_and_searchable; src/domain.rs::body_level_text_is_rendered_at_its_place_in_document_order | honoured | FIXED in the BV addendum, finished at A′. The vendored renderer walks the hierarchy, so the ECHR's closing signature block was lost from all three tools; the addendum appended what stands AFTER the last hierarchy child and prepended what stands AMONG it — which is not document order. A′ renders the latter at its place: before the line the next hierarchy sibling opens with, and `body_level_elements` says per element where it was rendered. The recorded ECHR consolidation proves the signature end to end; a synthetic document with a paragraph BETWEEN two articles proves the position, a shape no recorded manifestation carries. |
| `X19.1` | list_expressions | tests/e2e.rs::list_expressions_shows_pdf_only_before_a_read | honoured | The languages and formats of a version are answered from the graph, which knows every version rather than the one file. |
| `X19.3` | — | — | not_applicable | The server reads no identification block. |
| `X19.6` | get_references, parse_reference | tests/e2e.rs::linked_references_point_at_works_never_at_expressions | honoured | Test added at BV: every linked reference of the recorded BGÖ is a work-level ELI, and the coverage string says so. |
| `X19.8` | list_annexes | tests/e2e.rs::list_annexes_names_path_eids_that_read_article_reads | honoured | Every annex row carries its own work IRI beside the path eId. |
| `X19.9` | list_versions, check_in_force | tests/e2e.rs::the_bitemporal_loop_resolves_governing_versions_honestly | honoured | `list_versions` keeps future consolidations and `check_in_force` marks a future reference date with `future_as_of`. |

## 5. Not applicable to the base tier

These rules of the two rulebooks are **not** restated above: they
concern the semantic tier's chunking, hollowing, embedding or the four
RAG pipelines the analysis compared — E15's family cut puts all of that
in a sister component.

| id | why not (five words) |
|---|---|
| `X14.1` | chunk unit — indexing tier |
| `X14.2` | fallback chunk strategies — indexing tier |
| `X14.3` | chunk context metadata — indexing tier |
| `X14.4` | chunk special cases — indexing tier |
| `X20.1` | hollowing before chunking — indexing tier |
| `X20.2` | hollowing method — indexing tier |
| `X20.3` | chunking volume — indexing tier |
| `X10.1` | file-size dimensions for chunkers |
| `X10.3` | file sizes for chunkers |
| `X16.3` | nested lists for chunkers |
| `X18.6` | subdivision chunking — indexing tier |
| `X13.1` | collection profile for chunk strategy |
| `X13.2` | amendment chunking — indexing tier |
| `X13.3` | gazette chunking — indexing tier |
| `J9.2` | gazette relevance for RAG pipelines |
| `J10.4b` | consultation relevance for RAG pipelines |
| `J11.3b` | draft relevance for RAG pipelines |

And these carry a figure that supports another entry rather than a rule
of their own; they are folded, not dropped:

| id | folded into |
|---|---|
| `J4.5` | `J4.5b` |
| `J5.2` | `J5.1` |
| `J8.4` | `J6.1` |
| `J12.3` | `J12.1` |
| `X1.2b` | `X1.2` |
| `X2.1` | `X2.2` |
| `X4.1` | `X4.2` |
| `X4.4` | `X5.2` |
| `X4.5` | `X1.2` |
| `X6.2` | `X12.2` |
| `X7.1` | `X7.2` |
| `X8.1` | `X8.2` |
| `X8.3` | `X1.2` |
| `X9.1` | `X9.2` |
| `X11.1` | `X11.2` |
| `X12.1` | `X12.2` |
| `X15.2` | `X15.1` |
| `X16.1` | `X19.9` |
| `X16.2` | `X5.2` |
| `X16.5` | `X3.1` |
| `X16.6` | `X12.2` |
| `X17.2` | `X1.1` |
| `X17.4` | `X2.2` |
| `X17.5` | `X12.2` |
| `X17.8` | `X17.1` |
| `X17.9` | `X12.2` |
| `X19.2` | `X19.1` |
| `X19.4` | `X19.3` |
| `X19.5` | `X19.3` |
| `X19.7` | `X19.6` |

Re-checked at A′, because a folded rule must have no tool consequence
of its own: **X6.1** (the text elements — `<br/>` is in no separator
list), **X17.6** (several FRBR expressions per file) and **X18.2**
(four in five reference targets lie outside any local corpus) turned
out to HAVE one and now carry their own rows above. **J4.5** stays
folded into `J4.5b`: it measures why a subdivision count varies by
document type (296 for the StGB, 1 for many ordinances) where J4.5b
measures why it varies by query method — two grounds for one and the
same consequence, that a subdivision count is a gap catalogue and may
never be presented as an act's outline.

## 6. The five evaluation dimensions, and where each has a house

The analysis measured its four RAG systems along five dimensions. The
platform is not those systems — but the dimensions are the right
questions to ask of a domain server too, and four of the five already
have a home here. No new measurement was made for this page; this is a
map, not a result.

| dimension | the question | where it lives here | measured? |
|---|---|---|---|
| Retrieval effectiveness | does the system find the right places? | the LEXam probe (`testing/lexam-probe`) and the search proof `a_model_reaches_the_text_without_knowing_the_article_number` | partly — the probe runs, the search proof is a single case |
| Retrieval robustness | does the answer hold when the question is rephrased? | nothing yet: the fixtures pin one phrasing per case | not yet |
| Cost efficiency | tokens, latency, money per question | the E11 weight at the gateway (registry read 1, fedlex call 2), the manifestation cache (BO′) and the polite brake (BS) | partly — the weights are enforced and the cache is counted; latency is not tracked |
| Reproducibility | the same result on a repeated run | the recorded fixtures with semantic keys, and the offline suite that must pass without a network | yes, by construction |
| Data quality | are the answers formally valid? | this conformance table, the typed refusals, and every list's `truncated` flag with its original size | yes for the contract, partly for the data — that is what §4 measures |

The one dimension with no house is **robustness**: nothing in the tree
asks the same question twice in two wordings. It is named here so the
gap is visible, not so it is closed by naming it.

