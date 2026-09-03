//! The base-tier domain logic — TOOLSET-v0.md executed, extended by
//! the BQ navigator surface (TOOLSET-v1.md). Cross-cutting contract:
//! provenance mandatory (kind norm/hint, valid_as_of,
//! transaction_time), `as_of` echoed back resolved, honest typed
//! errors (not-found echoes its subject, false is a valid answer,
//! upstream failure never fabricates), injection refused by
//! construction (backend escaping), every list capped WITH a
//! `truncated` flag and the original size, never cut silently.
//!
//! Query patterns: developed LIVE at build time against the public
//! endpoint (single polite requests, recorded in the build report)
//! and frozen as fixtures — property names are the verified ones
//! (jolux:classifiedByTaxonomyEntry + skos:notation for SR,
//! jolux:isRealizedBy/language/title for titles, jolux:isMemberOf +
//! jolux:dateApplicability for consolidations incl. FUTURE ones,
//! jolux:inForceStatus / dateEntryInForce / dateDocument on the
//! abstract, jolux:foreseenImpactToLegalResource for the impact
//! graph).
//!
//! Two families of tools, two sources (ENGINE.md §1/§4):
//!
//! - **JOLux tools** run one or two SPARQL queries per call — the
//!   v0 spine by hand, the BQ additions THROUGH the vendored
//!   `fedlex-jolux` primitives (Apache-2.0, third_party/mcp-fedlex,
//!   PROVENANCE.md) over the [`KeyedClient`] bridge, plus two thin
//!   queries of our own where the vendored primitive answers a
//!   narrower question than the tool (taxonomy branch with all
//!   labels; manifestations of ONE version).
//! - **XML tools** resolve the version's Akoma-Ntoso manifestation
//!   (one SPARQL query, one fetch — re-fetched on EVERY call, there is
//!   no cache layer at the base tier by decision) and answer from the
//!   vendored `fedlex-akn` layer. On the recorded fixture they run
//!   fully offline.

use anyhow::Result;
use fedlex_akn::AknDocument;
use fedlex_core::{Eli, ValidAsOf};
use fedlex_jolux::{JoluxError, Language};
use serde_json::{json, Value};

use crate::backend::{
    busy_retry_after_ms, drive, iri_safe, sparql_escape, Backend, KeyedClient, Served,
};

const PREFIXES: &str = "PREFIX jolux: <http://data.legilux.public.lu/resource/ontology/jolux#>\n\
                        PREFIX skos: <http://www.w3.org/2004/02/skos/core#>\n";

/// Result caps (E16 discovery hygiene: an answer that does not fit a
/// context is not an answer). Every cap answers with `truncated` and
/// the original size.
const MAX_SEARCH_HITS: usize = 100;
/// How many words a title search may carry (BY point 0). Every word
/// becomes one `CONTAINS` in the filter, and the endpoint is shared:
/// a query longer than this is a pasted sentence, not a title, and it
/// is refused before a request rather than sent as a conjunction of
/// twenty tests.
const MAX_QUERY_WORDS: usize = 12;
const MAX_STRUCTURE_NODES: usize = 3000;
const MAX_DOCUMENT_CHARS: usize = 400_000;
const DEFAULT_DOCUMENT_CHARS: usize = 120_000;
const MAX_REFERENCES: usize = 1000;
const MAX_CHANGE_NOTES: usize = 500;
const MAX_ANNEX_ELEMENTS: usize = 200;
const MAX_RELATED: u32 = 50;
const MAX_VOCABULARY_MATCHES: u32 = 50;
/// The vendored `get_subdivisions` primitive caps at LIMIT 500; a
/// full page is reported as possibly truncated.
const SUBDIVISIONS_UPSTREAM_LIMIT: usize = 500;

/// Server context: backend + the resolved «today» (injected — the
/// domain never reads a clock; live main.rs injects the system date,
/// tests inject a fixed one).
pub struct Ctx {
    pub backend: Backend,
    pub today: String,
}

/// Typed domain refusals per the cross-cutting contract.
pub fn not_found(subject: &str) -> Value {
    json!({"error": "not-found", "subject": subject})
}
fn invalid(detail: &str) -> Value {
    json!({"error": "invalid-input", "detail": detail})
}
fn upstream(detail: String) -> Value {
    json!({"error": "upstream-unavailable", "detail": detail})
}

/// The fourth error kind (BS): the polite brake against the federal
/// endpoint is saturated and this request would have waited longer
/// than the limit. Not `upstream-unavailable` — the endpoint is fine,
/// WE are declining to hammer it — and not permanent: `retry_after_ms`
/// says when a retry finds a token. Built from the brake's own text,
/// which reaches here on both query paths (hand-written and bridged).
fn busy(text: &str) -> Value {
    let retry_after_ms = busy_retry_after_ms(text).unwrap_or(0);
    let detail = text
        .split("upstream-busy: retry_after_ms=")
        .nth(1)
        .and_then(|rest| rest.split_once(": "))
        .map(|(_, detail)| detail.to_string())
        .unwrap_or_else(|| text.to_string());
    json!({
        "error": "upstream-busy",
        "detail": detail,
        "retry_after_ms": retry_after_ms
    })
}

/// A backend error as the typed refusal it IS: the brake's refusal is
/// `upstream-busy` (with its retry), the endpoint's 4xx (a query the
/// WAF or parser rejected — almost always built from the caller's
/// input; a retry with the same value fails again) is `invalid-input`,
/// everything else `upstream-unavailable`.
fn backend_refusal(error: &anyhow::Error) -> Value {
    let text = format!("{error:#}");
    if busy_retry_after_ms(&text).is_some() {
        busy(&text)
    } else if text.contains("bad-request:") {
        invalid(&format!(
            "the endpoint rejected the query built from this input ({text}) — \
             rephrase with plain words, a retry with the same value fails again"
        ))
    } else {
        upstream(text)
    }
}

/// The bindings of a SELECT answer, or the typed upstream refusal a
/// malformed answer deserves (never an «internal» error).
macro_rules! bindings_or_refuse {
    ($value:expr) => {
        match Backend::bindings(&$value) {
            Ok(b) => b,
            Err(e) => return Ok(backend_refusal(&e)),
        }
    };
}

/// The languages a label is looked for in, in order (J5.3/J5.4). The
/// graph does NOT guarantee a German label — the vocabulary concept
/// `legal-subject-theme-fr/22158` carries only «Code» in French — so a
/// de-only read drops real concepts and real status labels.
const LABEL_FALLBACK: [&str; 5] = ["de", "en", "fr", "it", "rm"];

/// The label of a status IRI as the answer carries it: German first,
/// then English, French, Italian, Romansh — the catalogues without a
/// German label are real (J5.4), so a fallback is not a nicety (J5.3).
fn preferred_label(labels: &serde_json::Map<String, Value>) -> Option<String> {
    LABEL_FALLBACK
        .iter()
        .find_map(|lang| {
            labels
                .get(*lang)
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .or_else(|| labels.values().find_map(|v| v.as_str().map(str::to_string)))
}

/// Picks the label to answer with from what the graph HAS, and says
/// which language answered (J5.3/J5.4). Pure: the labels go in as
/// `(language tag, label)`, the choice comes out as `(label, language
/// tag)`. The caller's language first, then the fallback order; if the
/// graph has none of them, the first label it has (an answer in a
/// language nobody asked for beats no answer, and the tag says so).
fn choose_label(found: &[(String, String)], first: Option<&str>) -> Option<(String, String)> {
    let mut order: Vec<&str> = Vec::new();
    if let Some(f) = first {
        order.push(f);
    }
    order.extend(LABEL_FALLBACK.iter().copied().filter(|l| Some(*l) != first));
    for want in order {
        // Romansh is written «rm» and «roh» in the wild.
        let alt = match want {
            "rm" => Some("roh"),
            "roh" => Some("rm"),
            _ => None,
        };
        if let Some((lang, label)) = found
            .iter()
            .find(|(lang, _)| lang == want || Some(lang.as_str()) == alt)
        {
            return Some((label.clone(), lang.clone()));
        }
    }
    found.first().map(|(lang, label)| {
        (
            label.clone(),
            if lang.is_empty() {
                "und".to_string()
            } else {
                lang.clone()
            },
        )
    })
}

/// Every `skos:prefLabel` of one vocabulary IRI with its language tag —
/// ONE query for all languages, so the fallback of [`choose_label`]
/// costs no second request (J5.3/J5.4). The key is the IRI's, not the
/// caller's: the labels are a property of the concept.
fn vocabulary_labels(ctx: &Ctx, iri: &str) -> std::result::Result<Vec<(String, String)>, Value> {
    let query = format!(
        "{PREFIXES}SELECT ?label (LANG(?label) AS ?lang) WHERE {{\n\
         <{iri}> skos:prefLabel ?label .\n\
         }} LIMIT 20"
    );
    let value = ctx
        .backend
        .select(&format!("vocabulary_labels:{iri}"), &query)
        .map_err(|e| backend_refusal(&e))?;
    let bindings = Backend::bindings(&value).map_err(|e| upstream(format!("{e:#}")))?;
    Ok(bindings
        .iter()
        .filter_map(|b| {
            let label = binding_str(b, "label").filter(|s| !s.is_empty())?;
            Some((
                binding_str(b, "lang").unwrap_or("").to_ascii_lowercase(),
                label.to_string(),
            ))
        })
        .collect())
}

/// An ACT's ELI — and not a document of another collection (J9.3).
/// The Federal Gazette, the Official Compilation, its memorials and
/// the drafts are structurally unlike the classified compilation: they
/// carry no consolidations, no impacts and no citation edges. A
/// version, history or citation answer for one of them would be an
/// invented empty list, so the tools that answer those refuse it here,
/// before any request, and name the tool that does answer for it.
fn act_eli(eli: &str) -> std::result::Result<&str, Value> {
    let eli = match iri_safe(eli) {
        Ok(e) => e,
        Err(e) => return Err(invalid(&format!("{e:#}"))),
    };
    for (segment, what, instead) in [
        (
            "/eli/fga/",
            "a Federal Gazette document",
            "fedlex.get_fga_documents lists the gazette documents of an act",
        ),
        (
            "/eli/oc/",
            "an act of the Official Compilation",
            "fedlex.get_oc_act reads the AS entry of a consolidated act, fedlex.get_memorial its memorial",
        ),
        (
            "/eli/collection/",
            "an Official Compilation memorial",
            "fedlex.get_memorial takes it",
        ),
        (
            "/eli/dl/proj/",
            "a draft or a consultation",
            "fedlex.get_drafts, fedlex.get_consultations and fedlex.get_consultation_documents answer for it",
        ),
    ] {
        if eli.contains(segment) {
            return Err(invalid(&format!(
                "«{eli}» is {what}, not an act of the classified compilation — it carries no \
                 consolidations, impacts or citations; {instead}"
            )));
        }
    }
    Ok(eli)
}

/// Is the act in force at the date, by the act's own dates? ONE rule,
/// shared by check_in_force, get_law_metadata and resolve_sr (J3.1/J3.2,
/// the vendored JLX-TMP-03 shape): it started when the entry date is not
/// after the day, and it has not ended when neither end date is; where
/// no date is known the status vocabulary decides («…/0» = in force).
fn in_force_at(
    day: &str,
    entry: Option<&str>,
    no_longer: Option<&str>,
    end_applicability: Option<&str>,
    status: Option<&str>,
) -> bool {
    in_force_reason(day, entry, no_longer, end_applicability, status).0
}

/// The same rule, with the FIELD that decided (J3.2: an act may carry
/// two end dates — `dateNoLongerInForce` and `dateEndApplicability` —
/// and they disagree on about 4 % of expired acts; the EARLIER one
/// decides, and an answer that does not say which one it used leaves
/// the reader to guess between two dates in its own `dates` block).
fn in_force_reason(
    day: &str,
    entry: Option<&str>,
    no_longer: Option<&str>,
    end_applicability: Option<&str>,
    status: Option<&str>,
) -> (bool, &'static str) {
    // The earlier of the two end dates is the one that counts.
    let ended = [
        ("no_longer_in_force", no_longer),
        ("end_applicability", end_applicability),
    ]
    .into_iter()
    .filter_map(|(field, date)| date.filter(|d| *d <= day).map(|d| (d, field)))
    .min();
    match (entry, ended) {
        (_, Some((_, field))) => (false, field),
        (Some(entry), None) if entry <= day => (true, "entry_in_force"),
        (Some(_), None) => (false, "entry_in_force (not yet reached)"),
        (None, None) => match status {
            Some(s) if s.ends_with("/0") => (true, "status (no date in the graph)"),
            Some(_) => (false, "status (no date in the graph)"),
            None => (false, "nothing — the graph carries neither date nor status"),
        },
    }
}

fn provenance(ctx: &Ctx, valid_as_of: &str) -> Value {
    json!({
        "valid_as_of": valid_as_of,
        "transaction_time": ctx.today,
        "source": "fedlex.data.admin.ch/sparqlendpoint (live/base tier)"
    })
}

/// Provenance of an answer built from a fetched manifestation (BO′):
/// `transaction_time` is the moment the manifestation was REALLY
/// retrieved from the federal host — a cache hit keeps the original
/// retrieval moment instead of claiming today — and `served` says
/// live | cache | fixture. An extension of the v0 form: the three v0
/// fields stay exactly as they were.
fn provenance_served(ctx: &Ctx, loaded: &Loaded) -> Value {
    json!({
        "valid_as_of": loaded.date,
        "transaction_time": loaded.retrieved_at.clone().unwrap_or_else(|| ctx.today.clone()),
        "source": "fedlex.data.admin.ch/sparqlendpoint (live/base tier)",
        "served": loaded.served.as_str()
    })
}

fn binding_str<'a>(binding: &'a Value, var: &str) -> Option<&'a str> {
    binding
        .pointer(&format!("/{var}/value"))
        .and_then(Value::as_str)
}

fn binding_lang<'a>(binding: &'a Value, var: &str) -> Option<&'a str> {
    binding
        .pointer(&format!("/{var}/xml:lang"))
        .and_then(Value::as_str)
}

fn lang_code(uri_or_code: &str) -> String {
    match uri_or_code.rsplit('/').next().unwrap_or(uri_or_code) {
        "DEU" => "de".into(),
        "FRA" => "fr".into(),
        "ITA" => "it".into(),
        "ROH" => "rm".into(),
        "ENG" => "en".into(),
        other => other.to_ascii_lowercase(),
    }
}

// ---------------------------------------------------------------------
// Shared helpers for the BQ surface
// ---------------------------------------------------------------------

/// A full Fedlex IRI (already `iri_safe`) as the vendored crates'
/// relative `Eli` («eli/cc/…»).
fn relative_eli(full: &str) -> std::result::Result<Eli, Value> {
    let rel = full.strip_prefix(fedlex_jolux::FEDLEX_BASE).unwrap_or(full);
    Eli::new(rel).map_err(|e| invalid(&format!("{e}")))
}

/// Relative ELI («eli/cc/…») back to the full IRI this server speaks.
fn full_iri(rel: &str) -> String {
    if rel.starts_with("https://") {
        rel.to_string()
    } else {
        format!("{}{rel}", fedlex_jolux::FEDLEX_BASE)
    }
}

fn valid_as_of(date_iso: &str) -> std::result::Result<ValidAsOf, Value> {
    time::Date::parse(
        date_iso,
        &time::format_description::well_known::Iso8601::DATE,
    )
    .map(ValidAsOf::new)
    .map_err(|e| invalid(&format!("date «{date_iso}»: {e}")))
}

/// The vendored primitives' error → this server's typed refusal.
fn jolux_refusal(error: &JoluxError, subject: &str) -> Value {
    match error {
        JoluxError::NotFound(_) => not_found(subject),
        JoluxError::Id(e) => invalid(&format!("{e}")),
        JoluxError::BadRequest(status) => invalid(&format!(
            "the endpoint rejected the query built from this input (HTTP {status}) — \
             rephrase with plain words, a retry with the same value fails again"
        )),
        JoluxError::Transport(text) if busy_retry_after_ms(text).is_some() => busy(text),
        JoluxError::Transport(_) | JoluxError::MalformedResults(_) => upstream(format!("{error}")),
    }
}

/// Runs a vendored async primitive synchronously (see `backend::drive`).
fn run<F: std::future::Future<Output = std::result::Result<T, JoluxError>>, T>(
    future: F,
) -> std::result::Result<T, JoluxError> {
    drive(future).unwrap_or_else(|| {
        Err(JoluxError::Transport(
            "vendored primitive suspended on something other than the synchronous client".into(),
        ))
    })
}

/// Manifestation language: EU language-authority URIs (constants per
/// the vendored fedlex-jolux client.rs, Apache-2.0).
///
/// All five official languages of the vocabulary are accepted (J13.1).
/// WHICH of them a version actually carries as XML is the GRAPH's
/// answer, not a rule of this server: the recorded BGÖ 2023-11-01 has
/// XML in de, fr, it, rm AND en, and a version without XML in the
/// language asked answers not-found with that ground (load_version).
fn manifestation_lang(
    lang: Option<&str>,
) -> std::result::Result<(&'static str, &'static str), Value> {
    match lang.unwrap_or("de") {
        "de" => Ok(("de", Language::De.vocab_uri())),
        "fr" => Ok(("fr", Language::Fr.vocab_uri())),
        "it" => Ok(("it", Language::It.vocab_uri())),
        "en" => Ok(("en", Language::En.vocab_uri())),
        "rm" => Ok(("rm", Language::Roh.vocab_uri())),
        other => Err(invalid(&format!(
            "lang «{other}» must be de|fr|it|en|rm — which of them a version carries as XML is \
             the graph's answer (fedlex.list_expressions shows it before a read)"
        ))),
    }
}

fn label_lang(lang: Option<&str>) -> std::result::Result<Language, Value> {
    match lang.unwrap_or("de") {
        "de" => Ok(Language::De),
        "fr" => Ok(Language::Fr),
        "it" => Ok(Language::It),
        "en" => Ok(Language::En),
        "rm" => Ok(Language::Roh),
        other => Err(invalid(&format!("lang «{other}» must be de|fr|it|en|rm"))),
    }
}

/// A dated consolidation reference: `<abstract-eli>/<YYYYMMDD>`.
struct VersionRef<'a> {
    eli_version: &'a str,
    abstract_eli: &'a str,
    /// ISO date of the consolidation.
    date: String,
}

fn parse_version(eli_version: &str) -> std::result::Result<VersionRef<'_>, Value> {
    let eli_version = iri_safe(eli_version).map_err(|e| invalid(&format!("{e:#}")))?;
    match eli_version.rsplit_once('/') {
        Some((abstract_eli, d)) if d.len() == 8 && d.chars().all(|c| c.is_ascii_digit()) => {
            Ok(VersionRef {
                eli_version,
                abstract_eli,
                date: format!("{}-{}-{}", &d[0..4], &d[4..6], &d[6..8]),
            })
        }
        _ => Err(invalid(
            "eli_version must be <abstract-eli>/<YYYYMMDD> (a dated consolidation)",
        )),
    }
}

/// An Akoma-Ntoso eId, in its PATH form too (`art_2/para_1`,
/// `annex_u1/lvl_u1`): segments of `[A-Za-z0-9_.-]` joined by `/`.
/// Empty segments, `..` and whitespace are refused before any lookup.
fn valid_eid(eid: &str) -> std::result::Result<&str, Value> {
    let ok = !eid.is_empty()
        && eid.split('/').all(|segment| {
            !segment.is_empty()
                && segment != ".."
                && segment
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
        });
    if ok {
        Ok(eid)
    } else {
        Err(invalid(
            "eid must be an Akoma-Ntoso eId like «art_4» or a path like «art_2/para_1» \
             or «annex_u1/lvl_u1»",
        ))
    }
}

/// A resolved and parsed manifestation of one dated consolidation.
struct Loaded {
    doc: AknDocument,
    url: String,
    /// ISO date of the consolidation (its `valid_as_of`).
    date: String,
    as_of: ValidAsOf,
    lang: &'static str,
    /// How the manifestation reached this call (live | cache | fixture).
    served: Served,
    /// When it was really fetched (RFC 3339); `None` for fixtures.
    retrieved_at: Option<String>,
}

/// Version IRI → (abstract, date) → the FRBR manifestation chain
/// (query pattern adapted from the vendored fedlex-jolux resolve.rs:
/// isMemberOf → isRealizedBy → isEmbodiedBy → isExemplifiedBy,
/// incoming direction, xsd:date() constructor — the upstream
/// operational rules) → XML fetch → AknDocument parse.
///
/// Upstream reality stays honest: XML exists only since ~2021; older
/// versions are PDF-only and answer not-found with that ground
/// (`fedlex.list_expressions` shows it BEFORE a read).
///
/// Fixture keys are shared by every XML tool (`manifestation:…` and
/// `manifestation:xml:…`): the manifestation is a property of the
/// version, not of the tool that reads it.
fn load_version(
    ctx: &Ctx,
    eli_version: &str,
    lang: Option<&str>,
) -> Result<std::result::Result<Loaded, Value>> {
    let version = match parse_version(eli_version) {
        Ok(v) => v,
        Err(refusal) => return Ok(Err(refusal)),
    };
    let (lang_tag, lang_uri) = match manifestation_lang(lang) {
        Ok(l) => l,
        Err(refusal) => return Ok(Err(refusal)),
    };
    let as_of = match valid_as_of(&version.date) {
        Ok(d) => d,
        Err(refusal) => return Ok(Err(refusal)),
    };
    let key = format!("manifestation:{}:{lang_tag}", version.eli_version);
    // A cache line answers the whole chain — no query, no fetch — and
    // says so in the provenance (served: cache, the ORIGINAL
    // retrieval moment as transaction_time).
    if let Some(hit) = ctx.backend.cached_manifestation(&key) {
        let doc = match AknDocument::parse(&hit.body) {
            Ok(d) => d,
            Err(e) => {
                return Ok(Err(upstream(format!(
                    "cached manifestation does not parse as AKN: {e}"
                ))))
            }
        };
        return Ok(Ok(Loaded {
            doc,
            url: hit.url.clone(),
            date: version.date,
            as_of,
            lang: lang_tag,
            served: Served::Cache,
            retrieved_at: Some(hit.retrieved_at.clone()),
        }));
    }
    let query = format!(
        "{PREFIXES}PREFIX xsd: <http://www.w3.org/2001/XMLSchema#>\n\
         SELECT ?date ?url WHERE {{\n\
         ?cons jolux:isMemberOf <{}> ;\n\
               jolux:dateApplicability ?date ;\n\
               jolux:isRealizedBy ?expr .\n\
         ?expr jolux:language <{lang_uri}> ;\n\
               jolux:isEmbodiedBy ?manif .\n\
         ?manif jolux:isExemplifiedBy ?url .\n\
         FILTER(CONTAINS(STR(?url), \".xml\"))\n\
         FILTER(?date = xsd:date(\"{}\"))\n\
         }} LIMIT 1",
        version.abstract_eli, version.date
    );
    let value = match ctx.backend.select(&key, &query) {
        Ok(v) => v,
        Err(e) => return Ok(Err(backend_refusal(&e))),
    };
    let bindings = match Backend::bindings(&value) {
        Ok(b) => b,
        Err(e) => return Ok(Err(upstream(format!("{e:#}")))),
    };
    let Some(url) = bindings.first().and_then(|b| binding_str(b, "url")) else {
        return Ok(Err(json!({
            "error": "not-found",
            "subject": format!("{} ({lang_tag}) — no XML manifestation", version.eli_version),
            "detail": "XML manifestations exist only for recent consolidations; \
                       older versions are PDF-only (upstream reality, vendored \
                       fedlex-jolux J14.2) — fedlex.list_expressions shows the \
                       formats a version has"
        })));
    };
    let fetched = match ctx.backend.fetch_manifestation(
        &format!("manifestation:xml:{}:{lang_tag}", version.eli_version),
        url,
    ) {
        Ok(x) => x,
        Err(e) => return Ok(Err(backend_refusal(&e))),
    };
    let doc = match AknDocument::parse(&fetched.body) {
        Ok(d) => d,
        Err(e) => {
            return Ok(Err(upstream(format!(
                "manifestation does not parse as AKN: {e}"
            ))))
        }
    };
    // Parsed and sound: worth keeping (live backends only).
    ctx.backend.remember_manifestation(&key, url, &fetched);
    Ok(Ok(Loaded {
        doc,
        url: url.to_string(),
        date: version.date,
        as_of,
        lang: lang_tag,
        served: fetched.served,
        retrieved_at: fetched.retrieved_at,
    }))
}

/// Pulls the shared `Ok(Err(refusal))` shape into a plain `Value`.
macro_rules! loaded_or_refuse {
    ($expr:expr) => {
        match $expr? {
            Ok(loaded) => loaded,
            Err(refusal) => return Ok(refusal),
        }
    };
}

// ---------------------------------------------------------------------
// v0 spine (unchanged semantics)
// ---------------------------------------------------------------------

/// `fedlex.resolve_sr` — SR number → ELI with titles and status.
/// Real-data finding at build time: an SR number can match SEVERAL
/// abstracts (the current act AND repealed predecessors — 832.10
/// hits the 1994 KVG and the old cc/28 act). Disambiguation: rank
/// enforcement-status/0 (in force) first, else newest ELI; the
/// non-chosen matches stay VISIBLE as `also_matches` (E14
/// predecessor thinking — nothing is silently swallowed).
pub fn resolve_sr(ctx: &Ctx, sr: &str) -> Result<Value> {
    let sr = sr.trim();
    if sr.is_empty() || !sr.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return Ok(invalid("sr must be a systematic number like «832.10»"));
    }
    let escaped = sparql_escape(sr)?;
    // Candidates WITHOUT the title cross-product (an OPTIONAL join
    // plus LIMIT truncated multi-abstract results — found via the
    // recorded fixture).
    let query = format!(
        "{PREFIXES}SELECT DISTINCT ?abstract ?status ?statusLabel ?entryInForce ?noLonger ?endApp \
         WHERE {{\n\
         ?abstract a jolux:ConsolidationAbstract ;\n\
           jolux:classifiedByTaxonomyEntry ?entry .\n\
         ?entry skos:notation ?notation .\n\
         FILTER(STR(?notation) = \"{escaped}\")\n\
         OPTIONAL {{ ?abstract jolux:inForceStatus ?status\n\
           OPTIONAL {{ ?status skos:prefLabel ?statusLabel \
             FILTER(LANG(?statusLabel) IN (\"de\", \"en\", \"fr\", \"it\")) }} }}\n\
         OPTIONAL {{ ?abstract jolux:dateEntryInForce ?entryInForce }}\n\
         OPTIONAL {{ ?abstract jolux:dateNoLongerInForce ?noLonger }}\n\
         OPTIONAL {{ ?abstract jolux:dateEndApplicability ?endApp }}\n\
         }} LIMIT 48"
    );
    let value = match ctx
        .backend
        .select(&format!("resolve_sr:candidates:{sr}"), &query)
    {
        Ok(v) => v,
        Err(e) => return Ok(backend_refusal(&e)),
    };
    // One row per act: the status, its label in the first language the
    // vocabulary carries, and the three dates the in-force rule reads.
    #[derive(Default)]
    struct Candidate {
        status: String,
        labels: serde_json::Map<String, Value>,
        /// entry into force, no longer in force, end of applicability.
        dates: [Option<String>; 3],
    }
    let mut folded: std::collections::BTreeMap<String, Candidate> =
        std::collections::BTreeMap::new();
    for b in bindings_or_refuse!(value) {
        let Some(eli) = binding_str(b, "abstract") else {
            continue;
        };
        let row = folded.entry(eli.to_string()).or_default();
        if let Some(status) = binding_str(b, "status") {
            row.status = status.to_string();
        }
        if let Some(label) = binding_str(b, "statusLabel") {
            let lang = binding_lang(b, "statusLabel").unwrap_or("de").to_string();
            row.labels.entry(lang).or_insert(json!(label));
        }
        for (i, var) in ["entryInForce", "noLonger", "endApp"].iter().enumerate() {
            if row.dates[i].is_none() {
                row.dates[i] = binding_str(b, var).map(str::to_string);
            }
        }
    }
    let candidates: Vec<(String, String)> = folded
        .iter()
        .map(|(eli, row)| (eli.clone(), row.status.clone()))
        .collect();
    if candidates.is_empty() {
        return Ok(not_found(sr));
    }
    const IN_FORCE: &str = "https://fedlex.data.admin.ch/vocabulary/enforcement-status/0";
    let chosen = candidates
        .iter()
        .find(|(_, status)| status == IN_FORCE)
        .or_else(|| candidates.iter().max_by(|a, b| a.0.cmp(&b.0)))
        .cloned()
        .expect("non-empty");
    let mut profile = get_law_metadata(ctx, &chosen.0, None)?;
    if profile.get("error").is_some() {
        return Ok(profile);
    }
    let today = ctx.today.clone();
    let also: Vec<Value> = candidates
        .iter()
        .filter(|(eli, _)| *eli != chosen.0)
        .map(|(eli, status)| {
            let row = &folded[eli];
            let (labels, dates) = (&row.labels, &row.dates);
            json!({
                "eli": eli,
                "status": if status.is_empty() { Value::Null } else { json!(status) },
                "status_label": preferred_label(labels),
                "status_unset": status.is_empty(),
                "in_force": in_force_at(
                    &today,
                    dates[0].as_deref(),
                    dates[1].as_deref(),
                    dates[2].as_deref(),
                    if status.is_empty() { None } else { Some(status.as_str()) },
                ),
                "dates": {
                    "entry_in_force": dates[0],
                    "no_longer_in_force": dates[1],
                    "end_applicability": dates[2],
                },
            })
        })
        .collect();
    if let Some(map) = profile.as_object_mut() {
        map.insert("sr".into(), json!(sr));
        if !also.is_empty() {
            map.insert("also_matches".into(), json!(also));
        }
    }
    Ok(profile)
}

/// Does the query look like an official abbreviation («OR», «ZGB»,
/// «ArGV 1»)? Only then is the abbreviation pre-query worth its
/// request; long phrases are never abbreviations. Pattern from the
/// vendored fedlex-jolux `search.rs` (Apache-2.0, third_party/
/// mcp-fedlex — `looks_like_abbreviation`), rewritten here.
fn looks_like_abbreviation(query: &str) -> bool {
    let t = query.trim();
    !t.is_empty() && t.chars().count() <= 12 && t.split_whitespace().count() <= 2
}

/// The year segment of a consolidation-abstract ELI
/// (`…/eli/cc/2022/491` → 2022), for «newer first».
fn eli_year(eli: &str) -> u32 {
    eli.split('/')
        .skip_while(|seg| *seg != "cc" && *seg != "oc" && *seg != "fga")
        .nth(1)
        .and_then(|y| y.parse().ok())
        .unwrap_or(0)
}

const IN_FORCE_STATUS: &str = "https://fedlex.data.admin.ch/vocabulary/enforcement-status/0";

/// The systematic collection's own order as a rank: (number of
/// digits, the segments as numbers). A shorter SR number is the more
/// fundamental act — 832.10 (the KVG) before 832.102 (the KVV) before
/// 832.112.4; 235.1 (the DSG) before 235.11 (the DSV); treaties
/// (0.362.381.010) after domestic law. Within one depth the
/// collection's ascending order decides (832.10 before 832.12). An act
/// without an SR ranks after every act with one.
fn sr_rank(sr: Option<&str>) -> (usize, Vec<u64>) {
    match sr {
        Some(sr) => (
            sr.chars().filter(char::is_ascii_digit).count(),
            sr.split('.').map(|seg| seg.parse().unwrap_or(0)).collect(),
        ),
        None => (usize::MAX, Vec::new()),
    }
}

/// One search candidate, folded from the per-language rows.
#[derive(Default)]
struct SearchHit {
    titles: serde_json::Map<String, Value>,
    abbreviations: serde_json::Map<String, Value>,
    status: Option<String>,
    sr: Option<String>,
    /// 0 = exact abbreviation match, 1 = title/popular-name match.
    group: u8,
}

fn fold_search_rows(rows: &[Value], group: u8, into: &mut Vec<(String, SearchHit)>) {
    for b in rows {
        let Some(eli) = binding_str(b, "ca") else {
            continue;
        };
        let entry = match into.iter_mut().find(|(e, _)| e == eli) {
            Some((_, hit)) => hit,
            None => {
                into.push((
                    eli.to_string(),
                    SearchHit {
                        group,
                        ..Default::default()
                    },
                ));
                &mut into.last_mut().expect("just pushed").1
            }
        };
        if let (Some(lang), Some(title)) = (binding_str(b, "lang"), binding_str(b, "title")) {
            entry
                .titles
                .entry(lang_code(lang))
                .or_insert_with(|| json!(title));
        }
        if let (Some(lang), Some(short)) = (binding_str(b, "lang"), binding_str(b, "short")) {
            entry
                .abbreviations
                .entry(lang_code(lang))
                .or_insert_with(|| json!(short));
        }
        if let Some(status) = binding_str(b, "status") {
            entry.status.get_or_insert_with(|| status.to_string());
        }
        if let Some(sr) = binding_str(b, "sr") {
            entry.sr.get_or_insert_with(|| sr.to_string());
        }
    }
}

/// The preferred reading of a language map: de, fr, it, then whatever
/// the graph has.
fn preferred(map: &serde_json::Map<String, Value>) -> Option<(&str, &Value)> {
    ["de", "fr", "it", "en", "rm"]
        .iter()
        .find_map(|l| map.get(*l).map(|v| (*l, v)))
        .or_else(|| map.iter().next().map(|(k, v)| (k.as_str(), v)))
}

/// `fedlex.search_law` — title/abbreviation → ranked hint candidates.
///
/// Two ways in, one ranked list (BO′; pattern from the vendored
/// fedlex-jolux `search.rs`, Apache-2.0, third_party/mcp-fedlex —
/// abbreviation pre-query on `jolux:titleShort`, popular names via
/// `jolux:titleAlternative`, an inner DISTINCT window so the LIMIT
/// counts ACTS and not per-language rows; rewritten here against this
/// backend and with client-side ranking):
///
/// 1. Looks the query like an official abbreviation (≤ 12 chars, ≤ 2
///    words)? Then an exact, case-insensitive pre-query on
///    `jolux:titleShort` — «StPO» → `eli/cc/2010/267` — whose hits
///    rank first (group 0). Verified live at BO′ and recorded.
/// 2. The substring search over titles and popular names of every
///    language expression, as before — but the candidate window is
///    ordered in-force-first / newest-first BEFORE the limit cuts
///    (v0 took the first N rows the graph happened to return, which
///    for «krankenversicherung» were five repealed ordinances of
///    1965–1987 and not the KVG). **Every WORD of the query must be
///    found in the same title** (BY point 0), not the query as one
///    contiguous string: the graph writes an act's title with the
///    promulgation date interpolated — «Bundesgesetz vom 17. Dezember
///    1976 über die politischen Rechte» — so the official title as a
///    human writes it is never a substring of it. Asked for that
///    title, the search answered the UNO covenant and not the BPR
///    (the first live measurement, §6). For a one-word query the
///    filter is what it always was.
///
/// Every hit carries `status` (the enforcement-status IRI),
/// `in_force`, the titles and abbreviations the graph has per
/// language, and `sr` where the taxonomy provides it in the same
/// query (a cheap join on the ≤ 100 candidates). Ranking: group 0
/// first; in force first; newer ELI first; the ELI as tiebreak.
/// A hit stays a HINT, made binding only by a subsequent norm proof;
/// an empty result says what this search cannot do.
/// Every word of the query, as a case-insensitive substring, in the
/// SAME literal — the title filter of `search_law` (BY point 0).
///
/// A FUNCTION so the shape can be proven offline: the fixture backend
/// answers by semantic key and never reads the SPARQL, so a test over
/// a recorded window cannot tell this filter from the contiguous one
/// it replaces. For ONE word the FRAGMENT this builds is byte-for-byte
/// the older filter's — the `FILTER` line around it gained two
/// parenthesis pairs (123 → 127 bytes) — so a one-word query asks the
/// endpoint the same question, which is why the five recorded one-word
/// windows did not have to be re-recorded.
pub fn all_words_in(escaped_words: &[String], var: &str) -> String {
    escaped_words
        .iter()
        .map(|w| format!("CONTAINS(LCASE(STR(?{var})), \"{w}\")"))
        .collect::<Vec<_>>()
        .join(" && ")
}

pub fn search_law(ctx: &Ctx, query_text: &str, limit: Option<u32>) -> Result<Value> {
    let q = query_text.trim();
    if q.is_empty() {
        return Ok(invalid("query must be non-empty"));
    }
    let limit = limit.unwrap_or(10).clamp(1, 25);
    // BY point 0: the words, not the phrase. Each one must occur in
    // the SAME title — which is what «the title of this act» means
    // when the graph interpolates the promulgation date into it. A
    // one-word query asks the endpoint the same question as the old
    // filter — the fragment is byte-for-byte the same, inside a FILTER
    // that gained two parenthesis pairs — and for more words the match
    // set can only GROW: a title that carries the contiguous phrase
    // carries every word of it.
    let words: Vec<String> = q
        .to_lowercase()
        .split_whitespace()
        .map(str::to_string)
        .collect();
    if words.len() > MAX_QUERY_WORDS {
        return Ok(invalid(&format!(
            "a title search takes at most {MAX_QUERY_WORDS} words and this one has {}; every \
             word must occur in the same title, so name the distinctive ones («politischen \
             Rechte»), not a sentence",
            words.len()
        )));
    }
    let mut escaped_words: Vec<String> = Vec::with_capacity(words.len());
    for word in &words {
        escaped_words.push(sparql_escape(word)?);
    }
    let title_filter = all_words_in(&escaped_words, "ft");
    let alt_filter = all_words_in(&escaped_words, "alt");
    let mut folded: Vec<(String, SearchHit)> = Vec::new();

    // Group 0: the abbreviation pre-query (shared with parse_reference).
    let abbreviation_tried = looks_like_abbreviation(q);
    if abbreviation_tried {
        match abbreviation_hits(ctx, q)? {
            Ok(hits) => folded.extend(hits),
            Err(refusal) => return Ok(refusal),
        }
    }

    // Group 1: titles and popular names, candidate window ranked
    // BEFORE the limit (in force first, newest first), one row beyond
    // the window so `truncated` is measured.
    // At least forty acts wide, so the fundamental act of a field sits
    // in the window beside its many ordinances.
    let window = (limit * 4).clamp(40, 100) + 1;
    let query = format!(
        "{PREFIXES}SELECT ?ca ?lang ?title ?short ?status ?sr WHERE {{\n\
         {{ SELECT DISTINCT ?ca ?st WHERE {{\n\
            ?ca a jolux:ConsolidationAbstract ; jolux:isRealizedBy ?fe .\n\
            ?fe jolux:title ?ft .\n\
            OPTIONAL {{ ?fe jolux:titleAlternative ?alt }}\n\
            OPTIONAL {{ ?ca jolux:inForceStatus ?st }}\n\
            FILTER(({title_filter}) || (BOUND(?alt) && ({alt_filter})))\n\
         }} ORDER BY DESC(BOUND(?st)) ?st DESC(?ca) LIMIT {window} }}\n\
         ?ca jolux:isRealizedBy ?e . ?e jolux:language ?lang ; jolux:title ?title .\n\
         OPTIONAL {{ ?e jolux:titleShort ?short }}\n\
         OPTIONAL {{ ?ca jolux:inForceStatus ?status }}\n\
         OPTIONAL {{ ?ca jolux:classifiedByTaxonomyEntry ?t . ?t skos:notation ?sr }}\n\
         }}"
    );
    let value = match ctx
        .backend
        .select(&format!("search_law:{q}:{limit}"), &query)
    {
        Ok(v) => v,
        Err(e) => return Ok(backend_refusal(&e)),
    };
    let rows = bindings_or_refuse!(value);
    let before = folded.len();
    fold_search_rows(rows, 1, &mut folded);
    let title_candidates = folded.len() - before;
    let truncated = title_candidates as u32 > (window - 1) || folded.len() as u32 > limit;

    // Rank: abbreviation hits first; in force first; a status-less
    // stub after real acts; then the systematic collection's own
    // order (a shorter SR number is the more fundamental act — the
    // law before its ordinances — and within one depth the
    // collection's ascending order); then the newer ELI first.
    folded.sort_by(|(eli_a, a), (eli_b, b)| {
        let rank = |eli: &str, h: &SearchHit| {
            (
                h.group,
                if h.status.as_deref() == Some(IN_FORCE_STATUS) {
                    0
                } else {
                    1
                },
                u8::from(h.status.is_none()),
                sr_rank(h.sr.as_deref()),
                std::cmp::Reverse(eli_year(eli)),
                std::cmp::Reverse(eli.to_string()),
            )
        };
        rank(eli_a, a).cmp(&rank(eli_b, b))
    });
    let total_found = folded.len();
    folded.truncate(limit as usize);
    let hits: Vec<Value> = folded
        .iter()
        .map(|(eli, h)| {
            let (title_lang, title) = preferred(&h.titles)
                .map(|(l, v)| (l, v.clone()))
                .unwrap_or(("", Value::Null));
            json!({
                "eli": eli,
                "title": title,
                "title_lang": title_lang,
                "titles": h.titles,
                "abbreviation": preferred(&h.abbreviations).map(|(_, v)| v.clone()),
                "abbreviations": h.abbreviations,
                "status": h.status,
                // The shared rule (J3.1/J3.2): a search hit carries no
                // dates, so the status vocabulary decides — through
                // in_force_at, never by an IRI comparison of its own.
                "in_force": in_force_at(&ctx.today, None, None, None, h.status.as_deref()),
                "sr": h.sr,
                "matched": if h.group == 0 { "abbreviation" } else { "title" },
            })
        })
        .collect();
    let mut answer = json!({
        "query": q,
        "hits": hits,
        "returned": hits.len(),
        "found": total_found,
        "limit": limit,
        "truncated": truncated,
        "abbreviation_tried": abbreviation_tried,
        "kind": "hint",
        "provenance": provenance(ctx, &ctx.today)
    });
    if hits.is_empty() {
        answer["hint"] = json!(
            "no act carries this as an official abbreviation or in its title or popular name. \
             search_law is not a full-text search and knows no synonyms: for a question \
             about a matter, find the act by SR number (resolve_sr) or by a word of its \
             title, then search INSIDE it with search_text"
        );
    }
    // Hints, made binding only by a subsequent norm proof.
    Ok(answer)
}

/// `fedlex.get_law_metadata` — ELI → JOLux profile.
pub fn get_law_metadata(ctx: &Ctx, eli: &str, as_of: Option<&str>) -> Result<Value> {
    let eli = match iri_safe(eli) {
        Ok(e) => e,
        Err(e) => return Ok(invalid(&format!("{e:#}"))),
    };
    let valid_as_of = as_of.unwrap_or(&ctx.today);
    let query = format!(
        "{PREFIXES}SELECT ?lang ?title ?status ?statusLabel ?entryInForce ?noLonger ?endApp \
         ?docDate ?identifier WHERE {{\n\
         OPTIONAL {{ <{eli}> jolux:isRealizedBy ?e . ?e jolux:language ?lang ; jolux:title ?title }}\n\
         OPTIONAL {{ <{eli}> jolux:inForceStatus ?status\n\
           OPTIONAL {{ ?status skos:prefLabel ?statusLabel \
             FILTER(LANG(?statusLabel) IN (\"de\", \"en\", \"fr\", \"it\")) }} }}\n\
         OPTIONAL {{ <{eli}> jolux:dateEntryInForce ?entryInForce }}\n\
         OPTIONAL {{ <{eli}> jolux:dateNoLongerInForce ?noLonger }}\n\
         OPTIONAL {{ <{eli}> jolux:dateEndApplicability ?endApp }}\n\
         OPTIONAL {{ <{eli}> jolux:dateDocument ?docDate }}\n\
         OPTIONAL {{ <{eli}> <http://purl.org/dc/terms/identifier> ?identifier }}\n\
         }} LIMIT 48"
    );
    let value = match ctx
        .backend
        .select(&format!("get_law_metadata:{eli}"), &query)
    {
        Ok(v) => v,
        Err(e) => return Ok(backend_refusal(&e)),
    };
    let bindings = bindings_or_refuse!(value);
    let alive = bindings.iter().any(|b| {
        binding_str(b, "title").is_some()
            || binding_str(b, "status").is_some()
            || binding_str(b, "docDate").is_some()
    });
    if !alive {
        return Ok(not_found(eli));
    }
    let mut title = serde_json::Map::new();
    let mut dates = serde_json::Map::new();
    let mut status_labels = serde_json::Map::new();
    let mut status = Value::Null;
    let mut identifier = Value::Null;
    for b in bindings {
        if let (Some(lang), Some(t)) = (binding_str(b, "lang"), binding_str(b, "title")) {
            title.entry(lang_code(lang)).or_insert(json!(t));
        }
        if let Some(s) = binding_str(b, "status") {
            status = json!(s);
        }
        if let Some(label) = binding_str(b, "statusLabel") {
            let lang = binding_lang(b, "statusLabel").unwrap_or("de").to_string();
            status_labels.entry(lang).or_insert(json!(label));
        }
        for (var, field) in [
            ("entryInForce", "entry_in_force"),
            ("noLonger", "no_longer_in_force"),
            ("endApp", "end_applicability"),
            ("docDate", "document"),
        ] {
            if let Some(d) = binding_str(b, var) {
                dates.entry(field.to_string()).or_insert(json!(d));
            }
        }
        if let Some(i) = binding_str(b, "identifier") {
            identifier = json!(i);
        }
    }
    let date = |field: &str| dates.get(field).and_then(Value::as_str).map(str::to_string);
    let in_force = in_force_at(
        valid_as_of,
        date("entry_in_force").as_deref(),
        date("no_longer_in_force").as_deref(),
        date("end_applicability").as_deref(),
        status.as_str(),
    );
    Ok(json!({
        "eli": eli, "title": title, "status": status,
        "status_label": preferred_label(&status_labels),
        "status_unset": status.is_null(),
        "in_force": in_force,
        "dates": dates,
        "identifier": identifier, "kind": "norm",
        "note": "in_force is read from the act's own dates at valid_as_of — entry into force, and \
                 the earlier of no_longer_in_force and end_applicability (fedlex.check_in_force \
                 answers the same rule for any date); status_label decodes the status IRI in the \
                 first language the vocabulary carries (de, else en/fr/it/rm), status_unset says \
                 the act carries no status at all — 15 % of them do not. identifier is the \
                 Dublin-Core number the graph happens to carry (J16.1: a bare internal number, \
                 not the SR and not an ELI) — it is passed through, never to be built on",
        "provenance": provenance(ctx, valid_as_of)
    }))
}

/// Shared: the applicability-dated consolidations of an act,
/// ascending.
fn versions_of(ctx: &Ctx, eli: &str) -> Result<std::result::Result<Vec<(String, String)>, Value>> {
    let query = format!(
        "{PREFIXES}SELECT ?c ?applic WHERE {{\n\
         ?c jolux:isMemberOf <{eli}> ; jolux:dateApplicability ?applic\n\
         }} ORDER BY ?applic LIMIT 200"
    );
    let value = match ctx.backend.select(&format!("list_versions:{eli}"), &query) {
        Ok(v) => v,
        Err(e) => return Ok(Err(backend_refusal(&e))),
    };
    let bindings = match Backend::bindings(&value) {
        Ok(b) => b,
        Err(e) => return Ok(Err(upstream(format!("{e:#}")))),
    };
    let versions: Vec<(String, String)> = bindings
        .iter()
        .filter_map(|b| {
            Some((
                binding_str(b, "c")?.to_string(),
                binding_str(b, "applic")?.to_string(),
            ))
        })
        .collect();
    Ok(Ok(versions))
}

/// `fedlex.list_versions` — consolidations of an act (incl. future
/// ones — the graph carries them, honesty keeps them).
pub fn list_versions(ctx: &Ctx, eli: &str) -> Result<Value> {
    let eli = match act_eli(eli) {
        Ok(e) => e,
        Err(refusal) => return Ok(refusal),
    };
    let versions = match versions_of(ctx, eli)? {
        Ok(v) => v,
        Err(refusal) => return Ok(refusal),
    };
    // An act the graph knows but never consolidated answers an EMPTY
    // list — 6'532 acts are in that state (J3.3). Only an ELI the graph
    // knows nothing about is a not-found, and the profile decides which
    // of the two it is.
    if versions.is_empty() {
        let profile = get_law_metadata(ctx, eli, None)?;
        if profile.get("error").is_some() {
            return Ok(not_found(eli));
        }
        return Ok(json!({
            "versions": [],
            "total": 0,
            "kind": "norm",
            "note": "the graph knows this act (title and profile) but carries no consolidation \
                     for it — an empty list is the answer, not a missing one; fedlex.check_in_force \
                     answers from the act's own dates, fedlex.get_law_metadata shows the profile",
            "provenance": provenance(ctx, &ctx.today)
        }));
    }
    Ok(json!({
        "versions": versions.iter().map(|(v, d)| json!({
            "eli_version": v, "date": d
        })).collect::<Vec<_>>(),
        "total": versions.len(),
        "kind": "norm",
        "provenance": provenance(ctx, &ctx.today)
    }))
}

/// `fedlex.resolve_consolidation_at` — the governing version at a
/// date (the bitemporal core): max dateApplicability <= as_of.
pub fn resolve_consolidation_at(ctx: &Ctx, eli: &str, as_of: &str) -> Result<Value> {
    let eli = match act_eli(eli) {
        Ok(e) => e,
        Err(refusal) => return Ok(refusal),
    };
    if as_of.len() != 10 || as_of.as_bytes()[4] != b'-' {
        return Ok(invalid("as_of must be an ISO date YYYY-MM-DD"));
    }
    let versions = match versions_of(ctx, eli)? {
        Ok(v) => v,
        Err(refusal) => return Ok(refusal),
    };
    let governing = versions
        .iter()
        .filter(|(_, date)| date.as_str() <= as_of)
        .max_by(|a, b| a.1.cmp(&b.1));
    match governing {
        // Honest not-found: e.g. a date before first entry into force.
        None => Ok(not_found(&format!("{eli} @ {as_of}"))),
        Some((version, date)) => Ok(json!({
            "eli_version": version,
            "valid_as_of": date,
            "kind": "norm",
            "provenance": provenance(ctx, as_of)
        })),
    }
}

/// `fedlex.check_in_force` — in force at date? False is a VALID
/// answer, never an error.
pub fn check_in_force(ctx: &Ctx, eli: &str, as_of: &str) -> Result<Value> {
    let eli = match act_eli(eli) {
        Ok(e) => e,
        Err(refusal) => return Ok(refusal),
    };
    if as_of.len() != 10 || as_of.as_bytes()[4] != b'-' {
        return Ok(invalid("as_of must be an ISO date YYYY-MM-DD"));
    }
    let rel = match relative_eli(eli) {
        Ok(r) => r,
        Err(refusal) => return Ok(refusal),
    };
    let as_of_date = match valid_as_of(as_of) {
        Ok(d) => d,
        Err(refusal) => return Ok(refusal),
    };
    let client = KeyedClient::new(&ctx.backend, format!("check_in_force:{eli}"));
    let force = match run(fedlex_jolux::check_in_force(&client, &rel, as_of_date)) {
        Ok(response) => response.into_parts().0,
        Err(e) => return Ok(jolux_refusal(&e, eli)),
    };
    // J5.3: the vendored query reads the status label in German only.
    // Where the catalogue carries none, the label is fetched once for
    // the status IRI and the fallback order decides; status_label_lang
    // names the language the label is in, so a reader never mistakes a
    // French label for a German one.
    let mut status_label = force.current_status_label.clone();
    let mut status_label_lang = status_label.as_ref().map(|_| "de".to_string());
    if status_label.is_none() {
        if let Some(uri) = force.current_status_uri.as_deref() {
            match vocabulary_labels(ctx, uri) {
                Ok(found) => {
                    if let Some((label, lang)) = choose_label(&found, None) {
                        status_label = Some(label);
                        status_label_lang = Some(lang);
                    }
                }
                Err(refusal) => return Ok(refusal),
            }
        }
    }
    // The governing consolidation stays beside the answer — it is the
    // version a reader opens — but it no longer decides whether the act
    // is in force.
    let resolved = resolve_consolidation_at(ctx, eli, as_of)?;
    if resolved.get("error").is_some_and(|e| e != "not-found") {
        return Ok(resolved);
    }
    let governing = resolved.get("eli_version").cloned().unwrap_or(Value::Null);
    // An ELI the graph knows nothing about at all is a not-found — an
    // act it knows without any enforcement data is an ANSWER that says
    // so (J3.3: 15 % of the acts carry no status, 6'532 of them no
    // consolidation either).
    if force.no_enforcement_data && governing.is_null() {
        let profile = get_law_metadata(ctx, eli, None)?;
        if profile.get("error").is_some() {
            return Ok(not_found(eli));
        }
    }
    // J3.2: which of the act's own fields decided — the answer names it
    // instead of leaving a reader to compare two end dates by hand.
    let (_, decided_by) = in_force_reason(
        as_of,
        force.date_entry_in_force.as_deref(),
        force.date_no_longer_in_force.as_deref(),
        force.date_end_applicability.as_deref(),
        force.current_status_uri.as_deref(),
    );
    Ok(json!({
        "in_force": force.in_force,
        "decided_by": decided_by,
        "as_of": as_of,
        "dates": {
            "entry_in_force": force.date_entry_in_force,
            "no_longer_in_force": force.date_no_longer_in_force,
            "end_applicability": force.date_end_applicability,
        },
        "status": force.current_status_uri,
        "status_label": status_label,
        "status_label_lang": status_label_lang,
        "status_unset": force.current_status_uri.is_none(),
        "no_enforcement_data": force.no_enforcement_data,
        "governing_version": governing,
        // The domain never reads a clock: «today» is injected.
        "future_as_of": as_of > ctx.today.as_str(),
        "kind": "norm",
        "note": "in_force is decided by the ACT's own dates (vendored JLX-TMP-03): it started when \
                 dateEntryInForce <= as_of and it has not ended when neither dateNoLongerInForce \
                 nor dateEndApplicability is <= as_of — the EARLIER of the two counts, and they \
                 disagree on about 4 % of expired acts; where the act carries no date at all the \
                 status vocabulary decides, and no_enforcement_data: true means «the graph knows \
                 neither status nor date», not «out of force». status_label is read in German and, \
                 where the catalogue carries none, in en/fr/it/rm — status_label_lang says which. \
                 governing_version is the consolidation that governs the date; it does not decide \
                 the answer",
        "provenance": provenance(ctx, as_of)
    }))
}

/// `fedlex.get_citations` — v0 covers the PROVEN impact graph
/// (`jolux:foreseenImpactToLegalResource`, verified live at build);
/// in-text references are `fedlex.get_references` since BQ.
pub fn get_citations(ctx: &Ctx, eli: &str, direction: &str) -> Result<Value> {
    let eli = match act_eli(eli) {
        Ok(e) => e,
        Err(refusal) => return Ok(refusal),
    };
    // BV addendum (J16.1): the foreseen-impact field is thin — 2'598 of
    // 306'526 impacts (0.8 %), no type, no date — and what it actually
    // carries at the incoming end are the consultation drafts that
    // FORESEE an impact (the recorded KVG answer is 33 of them). The two
    // directions stay, because that is a real question the graph
    // answers; what changed is the promise: «who amended X» is
    // get_article_history, «who cites X» is direction cited_by.
    let (pattern, var) = match direction {
        "in" => (
            format!("?x jolux:foreseenImpactToLegalResource <{eli}>"),
            "x",
        ),
        "out" => (
            format!("<{eli}> jolux:foreseenImpactToLegalResource ?x"),
            "x",
        ),
        // BR: the formal citation graph — two more directions of the
        // SAME capability id (one tool, a typed direction: the v0
        // deviation + grounds, kept).
        "cites" | "cited_by" => return formal_citations(ctx, eli, direction),
        _ => {
            return Ok(invalid(
                "direction must be in | out (impacts) or cites | cited_by (formal citations)",
            ))
        }
    };
    let query = format!("{PREFIXES}SELECT DISTINCT ?{var} WHERE {{ {pattern} }} LIMIT 100");
    let value = match ctx
        .backend
        .select(&format!("get_citations:{eli}:{direction}"), &query)
    {
        Ok(v) => v,
        Err(e) => return Ok(backend_refusal(&e)),
    };
    let citations: Vec<Value> = bindings_or_refuse!(value)
        .iter()
        .filter_map(|b| binding_str(b, var))
        .map(|iri| match direction {
            "in" => json!({"from": iri, "to": eli}),
            _ => json!({"from": eli, "to": iri}),
        })
        .collect();
    Ok(json!({
        "citations": citations,
        "direction": direction,
        "coverage": "direction in|out reads jolux:foreseenImpactToLegalResource — the FORESEEN \
                     impact, filled on 2'598 of 306'526 impacts (0.8 %), carrying neither a type \
                     nor a date; at the incoming end it is mostly consultation drafts that foresee \
                     an impact on this act (the recorded KVG answer is 33 of them). «Who amended \
                     this act, when and how» is fedlex.get_article_history (the real impact graph, \
                     with type and date); «who cites it» is direction cited_by; in-text references \
                     are fedlex.get_references",
        "kind": "norm",
        "provenance": provenance(ctx, &ctx.today)
    }))
}

/// `fedlex.read_article` — eId-precise text of a version, bound to
/// the vendored Akoma-Ntoso layer (fedlex-akn, Apache-2.0 —
/// PROVENANCE.md): `load_version` → `get_element_text`. Accepts the
/// path eIds the structure and search tools hand out (`art_2/para_1`,
/// `annex_u1/lvl_u1`), so an annex is readable by the eId
/// `fedlex.list_annexes` names.
pub fn read_article(ctx: &Ctx, eli_version: &str, eid: &str, lang: Option<&str>) -> Result<Value> {
    let eid = match valid_eid(eid) {
        Ok(e) => e,
        Err(refusal) => return Ok(refusal),
    };
    let loaded = loaded_or_refuse!(load_version(ctx, eli_version, lang));
    let (duplicates, via_normalisation) = eid_resolution(&loaded.doc, eid);
    match fedlex_akn::get_element_text(&loaded.doc, eid, loaded.as_of) {
        Ok(response) => {
            let (element, _prov) = response.into_parts();
            Ok(json!({
                // BV A′: an XML answer says WHICH manifestation it
                // read — the version and its language. read_article
                // was the one that did not, and with five languages
                // accepted (J13.1) that is a gap a caller can trip on.
                "eli_version": eli_version,
                "lang": loaded.lang,
                "eid": element.eid,
                "eid_duplicates": duplicates,
                "eid_via_normalisation": via_normalisation,
                "element_kind": element.kind,
                "num": element.num,
                "heading": element.heading,
                "text": element.text,
                "notes": element.notes.len(),
                "section_path": element.section_path,
                "manifestation_url": loaded.url,
                "kind": "norm",
                "provenance": provenance_served(ctx, &loaded)
            }))
        }
        Err(e) => Ok(json!({
            "error": "not-found",
            "subject": format!("{eli_version}#{eid}"),
            "detail": format!("{e} — fedlex.get_structure lists the eIds this version has, \
                               fedlex.search_text finds the place by a word")
        })),
    }
}

// ---------------------------------------------------------------------
// BQ wave 1, A: XML tools (fedlex-akn over the version's manifestation)
// ---------------------------------------------------------------------

fn outline_to_json(node: &fedlex_akn::OutlineNode) -> Value {
    let mut map = serde_json::Map::new();
    if let Some(eid) = &node.eid {
        map.insert("eid".into(), json!(eid));
    }
    map.insert("kind".into(), json!(node.kind));
    if let Some(num) = &node.num {
        map.insert("num".into(), json!(num));
    }
    if let Some(heading) = &node.heading {
        map.insert("heading".into(), json!(heading));
    }
    if !node.children.is_empty() {
        map.insert(
            "children".into(),
            Value::Array(node.children.iter().map(outline_to_json).collect()),
        );
    }
    Value::Object(map)
}

/// Depth «article»: everything below an article (paragraphs, items)
/// is cut — the orientation view. LEVEL_BASED documents without
/// articles keep their tree (the level headings carry the meaning).
fn prune_below_articles(nodes: &mut [fedlex_akn::OutlineNode]) {
    for node in nodes.iter_mut() {
        if node.kind == "article" {
            node.children.clear();
        } else {
            prune_below_articles(&mut node.children);
        }
    }
}

fn count_nodes(nodes: &[fedlex_akn::OutlineNode]) -> usize {
    nodes.iter().map(|n| 1 + count_nodes(&n.children)).sum()
}

/// Keeps the first `budget` nodes in document order (pre-order), so a
/// capped outline is a PREFIX of the real one, never a sample.
fn cap_nodes(nodes: &mut Vec<fedlex_akn::OutlineNode>, budget: &mut usize) {
    let mut keep = 0;
    for node in nodes.iter_mut() {
        if *budget == 0 {
            break;
        }
        *budget -= 1;
        cap_nodes(&mut node.children, budget);
        keep += 1;
    }
    nodes.truncate(keep);
}

/// `fedlex.get_structure` — the outline of ONE dated consolidation:
/// sections/chapters/articles with eId, num and heading (vendored
/// AKN-STR-01). `depth`: «article» (default, the skeleton down to
/// article level) or «full» (the whole tree down to paragraphs).
/// Capped at [`MAX_STRUCTURE_NODES`] with `truncated` + `nodes_total`.
/// The tool that ends guessing article numbers.
pub fn get_structure(
    ctx: &Ctx,
    eli_version: &str,
    lang: Option<&str>,
    depth: Option<&str>,
) -> Result<Value> {
    let depth = match depth.unwrap_or("article") {
        "article" => "article",
        "full" => "full",
        other => return Ok(invalid(&format!("depth «{other}» must be article|full"))),
    };
    let loaded = loaded_or_refuse!(load_version(ctx, eli_version, lang));
    let (mut outline, _prov) =
        match fedlex_akn::get_document_structure(&loaded.doc, None, loaded.as_of) {
            Ok(r) => r.into_parts(),
            Err(e) => return Ok(upstream(format!("structure: {e}"))),
        };
    if depth == "article" {
        prune_below_articles(&mut outline);
    }
    let nodes_total = count_nodes(&outline);
    let mut budget = MAX_STRUCTURE_NODES;
    cap_nodes(&mut outline, &mut budget);
    let nodes_returned = count_nodes(&outline);
    let components = fedlex_akn::list_components(&loaded.doc).len();
    let extras = body_level_elements(&loaded.doc);
    Ok(json!({
        "eli_version": eli_version,
        "lang": loaded.lang,
        "depth": depth,
        "structure": outline.iter().map(outline_to_json).collect::<Vec<_>>(),
        "nodes_total": nodes_total,
        "nodes_returned": nodes_returned,
        "truncated": nodes_returned < nodes_total,
        "annexes": components,
        "body_level_elements": body_level_json(&extras),
        "note": if extras.is_empty() {
            Value::Null
        } else {
            json!(format!(
                "{} element(s) sit directly under <body> and outside this tree ({}) — they carry \
                 no eId, so fedlex.read_article cannot open them; their text is in \
                 fedlex.read_document and fedlex.search_text finds it (X17.8/X18.7)",
                extras.len(),
                extras.iter().map(|e| e.tag.as_str()).collect::<Vec<_>>().join(", ")
            ))
        },
        "manifestation_url": loaded.url,
        "kind": "norm",
        "provenance": provenance_served(ctx, &loaded)
    }))
}

/// The nearest self-or-ancestor article (or other hierarchy element
/// with a heading) of a node — the address a hit is READ by.
fn context_of(doc: &AknDocument, node: usize) -> (Option<String>, Option<String>) {
    let mut cur = Some(node);
    let mut fallback: Option<String> = None;
    while let Some(c) = cur {
        let tag = doc.tag(c);
        if fedlex_akn::is_hierarchy_tag(tag) {
            let num = doc.find_child(c, "num").map(|n| doc.text_of(n));
            let heading = doc.find_child(c, "heading").map(|h| doc.text_of(h));
            let label = [num, heading]
                .into_iter()
                .flatten()
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(" ");
            if tag == "article" {
                return (
                    doc.eid(c).map(str::to_string),
                    (!label.is_empty()).then_some(label),
                );
            }
            if fallback.is_none() && !label.is_empty() {
                fallback = Some(label);
            }
        }
        cur = doc.parent(c);
    }
    (None, fallback)
}

/// `fedlex.search_text` — case-insensitive substring search inside
/// ONE consolidation (vendored AKN-TXT-04 over the eId leaves). A hit
/// is a HINT: it names the eId and the article to read with
/// `fedlex.read_article`. `total` counts every hit, `truncated` says
/// whether `limit` cut the list.
pub fn search_text(
    ctx: &Ctx,
    eli_version: &str,
    query: &str,
    lang: Option<&str>,
    limit: Option<u32>,
) -> Result<Value> {
    let needle = query.trim();
    if needle.is_empty() {
        return Ok(invalid("query must be non-empty"));
    }
    let limit = limit.unwrap_or(20).clamp(1, MAX_SEARCH_HITS as u32) as usize;
    let loaded = loaded_or_refuse!(load_version(ctx, eli_version, lang));
    let outcome = fedlex_akn::search_text(&loaded.doc, needle, limit);
    // The vendored search walks eId-bearing leaves; the body-level
    // elements carry no eId and were invisible to it (X18.7). They are
    // searched here and answered with a null eId — the place is the
    // element's tag, and `read_article` cannot open it.
    let needle_lower = needle.to_lowercase();
    let extras = body_level_elements(&loaded.doc);
    let extra_hits: Vec<Value> = extras
        .iter()
        .filter(|e| e.text.to_lowercase().contains(&needle_lower))
        .map(|e| {
            let at = e.text.to_lowercase().find(&needle_lower).unwrap_or(0);
            let start = e.text[..at]
                .char_indices()
                .rev()
                .nth(40)
                .map_or(0, |(i, _)| i);
            let snippet: String = e.text[start..].chars().take(160).collect();
            json!({
                "eid": Value::Null,
                "element_kind": e.tag,
                "article_eid": Value::Null,
                "heading": Value::Null,
                "snippet": snippet.trim(),
                "note": "directly under <body>, outside the eId hierarchy — fedlex.read_document \
                         carries this text, fedlex.read_article cannot open it",
            })
        })
        .collect();
    let hits: Vec<Value> = outcome
        .hits
        .iter()
        .map(|hit| {
            let (article_eid, heading) = loaded
                .doc
                .lookup_eid(&hit.eid)
                .first()
                .map(|&node| context_of(&loaded.doc, node))
                .unwrap_or((None, None));
            json!({
                "eid": hit.eid,
                "element_kind": hit.kind,
                "article_eid": article_eid,
                "heading": heading,
                "snippet": hit.snippet,
            })
        })
        .collect();
    let extra_total = extra_hits.len();
    let mut hits = hits;
    hits.extend(extra_hits);
    hits.truncate(limit);
    Ok(json!({
        "eli_version": eli_version,
        "lang": loaded.lang,
        "query": needle,
        "hits": hits,
        "total": outcome.total + extra_total,
        "body_level_hits": extra_total,
        "truncated": outcome.total + extra_total > hits.len(),
        "limit": limit,
        "kind": "hint",
        "provenance": provenance_served(ctx, &loaded)
    }))
}

/// `fedlex.read_document` — the whole consolidation as readable
/// Markdown (vendored AKN-TXT-03; footnotes and formulas excluded),
/// capped in characters with `truncated`, the original length and a
/// continuation offset. For small acts and ordinances; quotations
/// come from `fedlex.read_article`.
pub fn read_document(
    ctx: &Ctx,
    eli_version: &str,
    lang: Option<&str>,
    max_chars: Option<u32>,
    offset: Option<u32>,
) -> Result<Value> {
    let max_chars = match max_chars {
        None => DEFAULT_DOCUMENT_CHARS,
        Some(0) => return Ok(invalid("max_chars must be at least 1")),
        Some(m) => (m as usize).min(MAX_DOCUMENT_CHARS),
    };
    let offset = offset.unwrap_or(0) as usize;
    let loaded = loaded_or_refuse!(load_version(ctx, eli_version, lang));
    let (markdown, _prov) = match fedlex_akn::get_readable_document(&loaded.doc, loaded.as_of) {
        Ok(r) => r.into_parts(),
        Err(e) => return Ok(upstream(format!("document: {e}"))),
    };
    // BV: the vendored renderer walks the hierarchy, so text that sits
    // DIRECTLY under <body> — a signature block, a stray paragraph or
    // table (X17.8/X18.7) — never reached the answer. It is rendered
    // here IN DOCUMENT ORDER: what stands after the last hierarchy
    // child at the end, what stands among them before the line its next
    // hierarchy sibling opens with. The note says which elements were
    // added, how many, and where each one went.
    let extras = body_level_elements(&loaded.doc);
    let markdown = render_body_level(markdown, &extras);
    let total_chars = markdown.chars().count();
    if offset > total_chars {
        return Ok(invalid(&format!(
            "offset {offset} lies beyond the document ({total_chars} characters)"
        )));
    }
    let window: String = markdown.chars().skip(offset).take(max_chars).collect();
    let end = offset + window.chars().count();
    let truncated = end < total_chars;
    Ok(json!({
        "eli_version": eli_version,
        "lang": loaded.lang,
        "markdown": window,
        "total_chars": total_chars,
        "offset": offset,
        "max_chars": max_chars,
        "truncated": truncated,
        "next_offset": if truncated { Value::from(end) } else { Value::Null },
        "manifestation_url": loaded.url,
        "body_level_elements": body_level_json(&extras),
        "note": if extras.is_empty() {
            "the whole document as the authentic text carries it".to_string()
        } else {
            format!(
                "{} element(s) sit directly under <body> and outside the hierarchy the renderer \
                 walks ({}) — their text IS included here, each at its place in document order: \
                 what stands after the last hierarchy child at the end, what stands among them \
                 before the element that follows it; body_level_elements names them with their \
                 length and where each one was rendered",
                extras.len(),
                extras
                    .iter()
                    .map(|e| e.tag.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        },
        "kind": "norm",
        "provenance": provenance_served(ctx, &loaded)
    }))
}

/// `eid` scope: the element itself or anything below it.
fn within_eid(candidate: Option<&str>, eid: &str) -> bool {
    candidate.is_some_and(|c| c == eid || c.starts_with(&format!("{eid}/")))
}

/// `fedlex.get_references` — the `<ref>` elements of a consolidation
/// (vendored AKN-REF-01: body, preamble and footnotes), each with its
/// source eId, the linked ELI where the corpus links it (70.8 %; 15 %
/// carry no href) and the visible label. A reference is a HINT — the
/// target is read with the norm tools. Optionally scoped to one eId.
pub fn get_references(
    ctx: &Ctx,
    eli_version: &str,
    eid: Option<&str>,
    lang: Option<&str>,
    limit: Option<u32>,
    offset: Option<u32>,
) -> Result<Value> {
    let eid = match eid {
        Some(e) => Some(match valid_eid(e) {
            Ok(e) => e,
            Err(refusal) => return Ok(refusal),
        }),
        None => None,
    };
    let limit = limit.unwrap_or(200).clamp(1, MAX_REFERENCES as u32) as usize;
    let offset = offset.unwrap_or(0) as usize;
    let loaded = loaded_or_refuse!(load_version(ctx, eli_version, lang));
    // The scope is the eId AS THE DOCUMENT WRITES IT: a caller may
    // hand in the JOLux form (`art_23a`) for an element the XML names
    // `art_23_a` — resolve first, then filter on the resolved name.
    let eid = match eid {
        Some(e) => match fedlex_akn::resolve_eid(&loaded.doc, e) {
            Ok(hit) => Some(loaded.doc.eid(hit.node).unwrap_or(e).to_string()),
            Err(_) => return Ok(not_found(&format!("{eli_version}#{e}"))),
        },
        None => None,
    };
    let eid = eid.as_deref();
    let (refs, _prov) = match fedlex_akn::get_all_references(&loaded.doc, loaded.as_of) {
        Ok(r) => r.into_parts(),
        Err(e) => return Ok(upstream(format!("references: {e}"))),
    };
    let scoped: Vec<_> = refs
        .into_iter()
        .filter(|r| eid.is_none_or(|e| within_eid(r.source_eid.as_deref(), e)))
        .collect();
    let total = scoped.len();
    // X11.2: a reference the corpus writes without a target is still a
    // reference — it is kept with a null href and counted, never
    // dropped to make the answer look complete.
    let unlinked = scoped.iter().filter(|r| unlinked_reference(r)).count();
    let page: Vec<Value> = scoped
        .iter()
        .skip(offset)
        .take(limit)
        .map(reference_json)
        .collect();
    let end = offset + page.len();
    Ok(json!({
        "eli_version": eli_version,
        "lang": loaded.lang,
        "eid": eid,
        "references": page,
        "total": total,
        "offset": offset,
        "limit": limit,
        "truncated": end < total,
        "next_offset": if end < total { Value::from(end) } else { Value::Null },
        "unlinked": unlinked,
        "coverage": "refs as the corpus links them: 70.8 % absolute Fedlex ELIs at work level, \
                     15 % carry no href (vendored fedlex-akn X11.2/X11.3) — those are kept, with \
                     a null href, and counted in `unlinked`. The FORMAL citation graph is \
                     fedlex.get_citations (directions cites|cited_by); in-text references and \
                     formal citations overlap by 0 to 48 %, so a complete picture needs both",
        "kind": "hint",
        "provenance": provenance_served(ctx, &loaded)
    }))
}

/// One reference row as the answer carries it — the href exactly as
/// the corpus writes it, `null` where it writes none (X11.2).
fn reference_json(r: &fedlex_akn::Reference) -> Value {
    json!({
        "source_eid": r.source_eid,
        "href": r.href,
        "label": r.label,
    })
}

/// Does this reference carry no target? (X11.2 — 15 % of the corpus's
/// references do not.)
fn unlinked_reference(r: &fedlex_akn::Reference) -> bool {
    r.href.as_deref().is_none_or(str::is_empty)
}

/// `fedlex.get_modifications` — the amendment record a consolidation
/// carries per element: the editorial change notes («Fassung gemäss
/// …», «Eingefügt durch …», with their AS/SR refs — vendored
/// AKN-MOD-02) anchored at their eIds, plus the `<mod>` blocks with
/// new wording an AMENDING act carries (AKN-MOD-01; empty on
/// consolidations, where they are already worked in). Optionally
/// scoped to one eId. A norm: it is what the authentic text says
/// about its own history.
pub fn get_modifications(
    ctx: &Ctx,
    eli_version: &str,
    eid: Option<&str>,
    lang: Option<&str>,
) -> Result<Value> {
    let eid = match eid {
        Some(e) => Some(match valid_eid(e) {
            Ok(e) => e,
            Err(refusal) => return Ok(refusal),
        }),
        None => None,
    };
    let loaded = loaded_or_refuse!(load_version(ctx, eli_version, lang));
    let (notes, _prov) = match fedlex_akn::extract_change_notes(&loaded.doc, eid, loaded.as_of) {
        Ok(r) => r.into_parts(),
        Err(fedlex_akn::AknError::EidNotFound(_)) => {
            return Ok(not_found(&format!(
                "{eli_version}#{}",
                eid.unwrap_or_default()
            )))
        }
        Err(e) => return Ok(upstream(format!("change notes: {e}"))),
    };
    let (mods, _prov) = match fedlex_akn::get_modifications(&loaded.doc, loaded.as_of) {
        Ok(r) => r.into_parts(),
        Err(e) => return Ok(upstream(format!("modifications: {e}"))),
    };
    // Scope by the document's own spelling of the eId (see
    // get_references) — the notes above were already resolved that
    // way inside the vendored primitive.
    let scope = eid.and_then(|e| {
        fedlex_akn::resolve_eid(&loaded.doc, e)
            .ok()
            .and_then(|hit| loaded.doc.eid(hit.node).map(str::to_string))
    });
    let mods: Vec<_> = mods
        .into_iter()
        .filter(|m| {
            scope.as_deref().is_none_or(|e| {
                within_eid(m.mod_eid.as_deref(), e) || within_eid(m.quoted_eid.as_deref(), e)
            })
        })
        .collect();
    let mods_total = mods.len();
    let notes_total = notes.len();
    let change_notes: Vec<Value> = notes
        .iter()
        .take(MAX_CHANGE_NOTES)
        .map(|n| {
            json!({
                "anchor_eid": n.anchor_eid,
                "marker": n.marker,
                "text": n.text,
                "refs": n.refs.iter().map(|r| json!({"href": r.href, "label": r.label})).collect::<Vec<_>>(),
            })
        })
        .collect();
    let mod_blocks: Vec<Value> = mods
        .iter()
        .take(MAX_CHANGE_NOTES)
        .map(|m| {
            json!({
                "mod_eid": m.mod_eid,
                "quoted_root_kind": m.quoted_root_kind,
                "quoted_eid": m.quoted_eid,
                "new_text": m.new_text,
            })
        })
        .collect();
    Ok(json!({
        "eli_version": eli_version,
        "lang": loaded.lang,
        "eid": eid,
        "change_notes": change_notes,
        "change_notes_total": notes_total,
        "truncated": notes_total > change_notes.len(),
        "mod_blocks": mod_blocks,
        "mod_blocks_total": mods_total,
        "mod_blocks_truncated": mods_total > mod_blocks.len(),
        "coverage": "change notes = the authorialNote footnotes of the authentic text \
                     (71.3 % carry AS/SR refs); mod blocks exist on amending acts only. \
                     The JOLux impact graph is fedlex.get_article_history",
        "kind": "norm",
        "provenance": provenance_served(ctx, &loaded)
    }))
}

/// `fedlex.list_annexes` — the annexes of a consolidation as the
/// Akoma-Ntoso `<component>` documents carry them (vendored
/// AKN-CMP-01/02): title, own work IRI, whether the body is a stub,
/// and the PATH eIds of the top-level elements (`annex_u1/lvl_u1`)
/// that `fedlex.read_article` reads directly.
pub fn list_annexes(ctx: &Ctx, eli_version: &str, lang: Option<&str>) -> Result<Value> {
    let loaded = loaded_or_refuse!(load_version(ctx, eli_version, lang));
    let infos = fedlex_akn::list_components(&loaded.doc);
    let mut annexes = Vec::with_capacity(infos.len());
    for info in &infos {
        let (heading, elements, elements_total) =
            match fedlex_akn::get_component_document(&loaded.doc, info.index) {
                Ok(sub) => {
                    let heading = sub
                        .find_child(sub.root(), "preface")
                        .map(|p| sub.text_of(p))
                        .filter(|s| !s.is_empty());
                    let outline = fedlex_akn::get_document_structure(&sub, None, loaded.as_of)
                        .map(|r| r.into_parts().0)
                        .unwrap_or_default();
                    let mut flat = outline;
                    prune_below_articles(&mut flat);
                    let elements: Vec<Value> = flat
                        .iter()
                        .take(MAX_ANNEX_ELEMENTS)
                        .map(|n| {
                            json!({
                                "eid": n.eid, "kind": n.kind, "num": n.num, "heading": n.heading,
                                "children": count_nodes(&n.children),
                            })
                        })
                        .collect();
                    (heading, elements, flat.len())
                }
                Err(e) => (Some(format!("unreadable component: {e}")), Vec::new(), 0),
            };
        let elements_len = elements.len();
        let eid_prefix = elements
            .first()
            .and_then(|e| e["eid"].as_str())
            .and_then(|eid| eid.split('/').next())
            .map(str::to_string);
        annexes.push(json!({
            "index": info.index,
            "doc_name": info.doc_name,
            "eli_work": info.eli_work,
            "title": info.title,
            "heading": heading,
            "is_empty_stub": info.is_empty_stub,
            "eid_prefix": eid_prefix,
            "elements": elements,
            "elements_total": elements_total,
            "elements_truncated": elements_total > elements_len,
        }));
    }
    Ok(json!({
        "eli_version": eli_version,
        "lang": loaded.lang,
        "annexes": annexes,
        "total": infos.len(),
        "note": "annexes are <component> documents of the manifestation; read an element by its \
                 path eId with fedlex.read_article. The JOLux graph's own annex view (only annexes \
                 with amendments) is fedlex.get_subdivisions",
        "kind": "norm",
        "provenance": provenance_served(ctx, &loaded)
    }))
}

// ---------------------------------------------------------------------
// BQ wave 1, B: JOLux tools (fedlex-jolux primitives, live graph)
// ---------------------------------------------------------------------

/// `fedlex.get_article_history` — which amendments acted on ONE
/// element of an act, with the consolidation each opens.
///
/// Two short queries of our own, in the vendored JLX-IMP-02 shape
/// (the main query «from»-free, the source query short — the federal
/// WAF blocks long queries carrying «SELECT … from», see fedlex-jolux
/// impacts.rs) — but with an EXACT target: the vendored primitive
/// filters `CONTAINS(STR(?target), "<eid>")`, so `art_2` also
/// collected the impacts of `art_20` … `art_23a` (found at the BQ
/// review on the recorded BGÖ answer). Here the target must be the
/// element itself or a descendant of it. The eId is normalised to the
/// JOLux form (`art_14_a` → `art_14a`) first.
///
/// Each impact is joined to the consolidation whose applicability
/// date it opens, so the answer names (date, version). The
/// completeness caveat travels IN the answer: since 2023 Fedlex often
/// names affected articles only in the free-text comment of the
/// act-level impact, so an empty list never proves «never amended».
pub fn get_article_history(ctx: &Ctx, eli: &str, eid: &str) -> Result<Value> {
    let eli = match act_eli(eli) {
        Ok(e) => e,
        Err(refusal) => return Ok(refusal),
    };
    let eid = match valid_eid(eid) {
        Ok(e) => fedlex_core::normalize_eid(e),
        Err(refusal) => return Ok(refusal),
    };
    let target = format!("{eli}/{eid}");
    let main = format!(
        "{PREFIXES}SELECT DISTINCT ?impact ?type ?typeLabel ?date ?comment WHERE {{\n\
         ?impact jolux:impactToLegalResource ?target .\n\
         OPTIONAL {{ ?impact jolux:legalResourceImpactHasType ?type\n\
           OPTIONAL {{ ?type skos:prefLabel ?typeLabel . FILTER(LANG(?typeLabel) = \"de\") }} }}\n\
         OPTIONAL {{ ?impact jolux:legalResourceImpactHasDateEntryInForce ?date }}\n\
         OPTIONAL {{ ?impact jolux:impactToLegalResourceComment ?comment }}\n\
         FILTER(?target = <{target}> || STRSTARTS(STR(?target), \"{target}/\"))\n\
         }} ORDER BY ?date LIMIT 201"
    );
    let key = format!("get_article_history:{eli}:{eid}");
    let value = match ctx.backend.select(&key, &main) {
        Ok(v) => v,
        Err(e) => return Ok(backend_refusal(&e)),
    };
    let bindings = bindings_or_refuse!(value);
    // Rows collapse per impact (a comment or label can fan out).
    let mut impacts: Vec<ImpactRow> = Vec::new();
    for b in bindings {
        let Some(uri) = binding_str(b, "impact") else {
            continue;
        };
        if impacts.iter().any(|row| row.0 == uri) {
            continue;
        }
        let nonempty = |v: &str| Some(binding_str(b, v).filter(|x| !x.is_empty())?.to_string());
        impacts.push((
            uri.to_string(),
            nonempty("date"),
            nonempty("type"),
            nonempty("typeLabel"),
            nonempty("comment"),
        ));
    }
    // One row beyond the cap makes `truncated` a measurement.
    let truncated = impacts.len() > 200;
    impacts.truncate(200);
    // The amending acts, from the short second query.
    let mut sources: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    if !impacts.is_empty() {
        let src = format!(
            "{PREFIXES}SELECT DISTINCT ?impact ?src WHERE {{\n\
             ?impact jolux:impactToLegalResource ?target ;\n\
                     jolux:impactFromLegalResource ?src .\n\
             FILTER(?target = <{target}> || STRSTARTS(STR(?target), \"{target}/\"))\n\
             }} LIMIT 201"
        );
        let value = match ctx.backend.select(&format!("{key}:sources"), &src) {
            Ok(v) => v,
            Err(e) => return Ok(backend_refusal(&e)),
        };
        for b in bindings_or_refuse!(value) {
            if let (Some(i), Some(from)) = (binding_str(b, "impact"), binding_str(b, "src")) {
                sources
                    .entry(i.to_string())
                    .or_insert_with(|| from.to_string());
            }
        }
    }
    let versions = match versions_of(ctx, eli)? {
        Ok(v) => v,
        Err(refusal) => return Ok(refusal),
    };
    if versions.is_empty() {
        return Ok(not_found(eli));
    }
    let impacts: Vec<Value> = impacts
        .iter()
        .map(|(uri, date, kind, label, comment)| {
            let version = date.as_deref().and_then(|d| {
                versions
                    .iter()
                    .find(|(_, applic)| applic == d)
                    .map(|(v, _)| v.clone())
            });
            json!({
                "impact_uri": uri,
                "date": date,
                "version": version,
                "type": kind,
                "type_label": label,
                "from": sources.get(uri),
                "comment": comment,
            })
        })
        .collect();
    Ok(json!({
        "eli": eli,
        "eid": eid,
        "target": target,
        "impacts": impacts,
        "total": impacts.len(),
        "truncated": truncated,
        "completeness_note": "may be incomplete: since the 2023 system change Fedlex often names \
                              affected articles only in the free-text comment of the act-level \
                              impact — fedlex.get_modifications shows the change notes the \
                              authentic text itself carries",
        "kind": "norm",
        "provenance": provenance(ctx, &ctx.today)
    }))
}

/// One impact as collected from the bindings: uri, date, type IRI,
/// type label, comment.
type ImpactRow = (
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
);

/// `fedlex.get_subdivisions` — the subdivisions the JOLux graph knows
/// for an act (vendored JLX-SUB-01, transitive). A GAP CATALOGUE, not
/// an outline: only elements with at least one amendment exist as
/// subdivisions (0.4–8.5 % of the eIds); `fedlex.get_structure` is the
/// outline.
pub fn get_subdivisions(ctx: &Ctx, eli: &str) -> Result<Value> {
    let eli = match act_eli(eli) {
        Ok(e) => e,
        Err(refusal) => return Ok(refusal),
    };
    let rel = match relative_eli(eli) {
        Ok(r) => r,
        Err(refusal) => return Ok(refusal),
    };
    let as_of = match valid_as_of(&ctx.today) {
        Ok(d) => d,
        Err(refusal) => return Ok(refusal),
    };
    let client = KeyedClient::new(&ctx.backend, format!("get_subdivisions:{eli}"));
    let subs = match run(fedlex_jolux::get_subdivisions(&client, &rel, as_of, None)) {
        Ok(response) => response.into_parts().0,
        Err(e) => return Ok(jolux_refusal(&e, eli)),
    };
    if subs.is_empty() {
        // An act without amended elements answers an empty catalogue;
        // an act the graph does not know answers not-found. The
        // profile tells the two apart.
        let profile = get_law_metadata(ctx, eli, None)?;
        if profile.get("error").is_some() {
            return Ok(profile);
        }
    }
    let subdivisions: Vec<Value> = subs
        .iter()
        .map(|s| {
            let eid = s.uri.strip_prefix(&format!("{eli}/")).map(str::to_string);
            json!({"uri": s.uri, "eid": eid, "type": s.subdivision_type})
        })
        .collect();
    Ok(json!({
        "eli": eli,
        "subdivisions": subdivisions,
        "total": subdivisions.len(),
        // The vendored primitive caps at LIMIT 500 and cannot ask for
        // one more; a full page is reported as possibly cut, with the
        // basis named.
        "truncated": subdivisions.len() >= SUBDIVISIONS_UPSTREAM_LIMIT,
        "cap": SUBDIVISIONS_UPSTREAM_LIMIT,
        "truncation_basis": "cap reached (the vendored primitive's LIMIT 500) — not a count",
        // J17.3: the number depends on the walk, so the walk is named.
        "walk": "transitive (the vendored JLX-SUB-01 shape: the act's subdivisions AND the \
                 subdivisions of those) — a direct-children walk answers a smaller number for the \
                 same act, which is why two subdivision counts are comparable only with the walk \
                 beside them",
        "note": "gap catalogue: JOLux knows only elements with at least one amendment — \
                 the outline is fedlex.get_structure",
        "kind": "norm",
        "provenance": provenance(ctx, &ctx.today)
    }))
}

/// One taxonomy entry as collected from the bindings: notation,
/// labels by language tag, `skos:broader` parent.
type TaxonomyNode = (
    Option<String>,
    serde_json::Map<String, Value>,
    Option<String>,
);

/// `fedlex.get_taxonomy` — the systematic classification of an act:
/// its taxonomy entries with notation and labels in every language
/// the vocabulary carries, and the `skos:broader` chain up to the SR
/// branch. One query of our own (the vendored JLX-TAX-01 answers one
/// language and no notation; the branch chain is a property path
/// over the same predicates it uses).
pub fn get_taxonomy(ctx: &Ctx, eli: &str) -> Result<Value> {
    let eli = match iri_safe(eli) {
        Ok(e) => e,
        Err(e) => return Ok(invalid(&format!("{e:#}"))),
    };
    let query = format!(
        "{PREFIXES}SELECT ?tax ?node ?notation ?label ?parent WHERE {{\n\
         <{eli}> jolux:classifiedByTaxonomyEntry ?tax .\n\
         ?tax skos:broader* ?node .\n\
         OPTIONAL {{ ?node skos:notation ?notation }}\n\
         OPTIONAL {{ ?node skos:prefLabel ?label }}\n\
         OPTIONAL {{ ?node skos:broader ?parent }}\n\
         }} LIMIT 301"
    );
    let value = match ctx.backend.select(&format!("get_taxonomy:{eli}"), &query) {
        Ok(v) => v,
        Err(e) => return Ok(backend_refusal(&e)),
    };
    let bindings = bindings_or_refuse!(value);
    // Entry nodes (uri → notation, labels by language, parent).
    // One row beyond the cap makes `truncated` a measurement.
    let truncated = bindings.len() > 300;
    let bindings = &bindings[..bindings.len().min(300)];
    let mut nodes: std::collections::BTreeMap<String, TaxonomyNode> =
        std::collections::BTreeMap::new();
    let mut leaves: Vec<String> = Vec::new();
    for b in bindings {
        let (Some(tax), Some(node)) = (binding_str(b, "tax"), binding_str(b, "node")) else {
            continue;
        };
        if !leaves.iter().any(|l| l == tax) {
            leaves.push(tax.to_string());
        }
        let entry = nodes.entry(node.to_string()).or_default();
        if let Some(n) = binding_str(b, "notation") {
            entry.0.get_or_insert_with(|| n.to_string());
        }
        if let Some(l) = binding_str(b, "label") {
            let lang = binding_lang(b, "label").unwrap_or("und");
            entry.1.entry(lang.to_string()).or_insert(json!(l));
        }
        if let Some(p) = binding_str(b, "parent") {
            entry.2.get_or_insert_with(|| p.to_string());
        }
    }
    if leaves.is_empty() {
        // Honest: an unclassified act (about 10'000 of them) versus an
        // unknown one — distinguish by the profile.
        let profile = get_law_metadata(ctx, eli, None)?;
        if profile.get("error").is_some() {
            return Ok(profile);
        }
        return Ok(json!({
            "eli": eli, "entries": [], "branches": [],
            "note": "this act carries no taxonomy entry (about 10'000 consolidation abstracts are unclassified)",
            "kind": "norm", "provenance": provenance(ctx, &ctx.today)
        }));
    }
    let entry_json = |uri: &str| -> Value {
        let (notation, labels, parent) = nodes.get(uri).cloned().unwrap_or_default();
        json!({"uri": uri, "notation": notation, "labels": labels, "broader": parent})
    };
    // The chain from the SR branch (root) down to each leaf entry.
    let branches: Vec<Value> = leaves
        .iter()
        .map(|leaf| {
            let mut chain = Vec::new();
            let mut cur = Some(leaf.clone());
            let mut guard = 0;
            while let Some(uri) = cur {
                chain.push(entry_json(&uri));
                cur = nodes.get(&uri).and_then(|n| n.2.clone());
                guard += 1;
                if guard > 32 {
                    break;
                }
            }
            chain.reverse();
            json!({"entry": leaf, "chain": chain})
        })
        .collect();
    let entries: Vec<Value> = leaves.iter().map(|l| entry_json(l)).collect();
    Ok(json!({
        "eli": eli,
        "entries": entries,
        "branches": branches,
        "truncated": truncated,
        "kind": "norm",
        "provenance": provenance(ctx, &ctx.today)
    }))
}

fn format_of(url: &str) -> &'static str {
    let lower = url.to_ascii_lowercase();
    if lower.contains(".xml") {
        "xml"
    } else if lower.contains(".pdf") {
        "pdf"
    } else if lower.contains(".html") || lower.contains(".htm") {
        "html"
    } else if lower.contains(".docx") || lower.contains(".doc") {
        "docx"
    } else {
        "other"
    }
}

/// `fedlex.list_expressions` — the language versions of ONE dated
/// consolidation and the manifestations each has (XML, PDF, HTML,
/// DOCX with their URLs). The tool that shows «PDF-only» BEFORE a
/// read: `xml_available` per language, `pdf_only` for the version.
/// One query of our own on the version node (the vendored JLX-RES-05
/// lists languages across ALL consolidations of the act and says
/// nothing about formats).
pub fn list_expressions(ctx: &Ctx, eli_version: &str) -> Result<Value> {
    let version = match parse_version(eli_version) {
        Ok(v) => v,
        Err(refusal) => return Ok(refusal),
    };
    let query = format!(
        "{PREFIXES}SELECT ?lang ?url WHERE {{\n\
         <{}> jolux:isRealizedBy ?expr .\n\
         ?expr jolux:language ?lang .\n\
         OPTIONAL {{ ?expr jolux:isEmbodiedBy ?m . ?m jolux:isExemplifiedBy ?url }}\n\
         }} LIMIT 101",
        version.eli_version
    );
    let value = match ctx
        .backend
        .select(&format!("list_expressions:{}", version.eli_version), &query)
    {
        Ok(v) => v,
        Err(e) => return Ok(backend_refusal(&e)),
    };
    let bindings = bindings_or_refuse!(value);
    if bindings.is_empty() {
        return Ok(not_found(version.eli_version));
    }
    let truncated = bindings.len() > 100;
    let bindings = &bindings[..bindings.len().min(100)];
    let mut by_lang: std::collections::BTreeMap<String, Vec<(String, String)>> =
        std::collections::BTreeMap::new();
    for b in bindings {
        let Some(lang) = binding_str(b, "lang") else {
            continue;
        };
        let code = Language::from_vocab_uri(lang)
            .map(|l| l.tag().to_string())
            .unwrap_or_else(|| lang.to_string());
        let list = by_lang.entry(code).or_default();
        if let Some(url) = binding_str(b, "url") {
            let pair = (format_of(url).to_string(), url.to_string());
            if !list.contains(&pair) {
                list.push(pair);
            }
        }
    }
    let mut any_xml = false;
    let mut manifestations_total = 0usize;
    let languages: Vec<Value> = by_lang
        .iter()
        .map(|(code, manifs)| {
            let xml = manifs.iter().any(|(f, _)| f == "xml");
            any_xml |= xml;
            manifestations_total += manifs.len();
            let mut formats: Vec<&str> = manifs.iter().map(|(f, _)| f.as_str()).collect();
            formats.sort_unstable();
            formats.dedup();
            json!({
                "lang": code,
                "formats": formats,
                "xml_available": xml,
                "manifestations": manifs.iter().map(|(f, u)| json!({"format": f, "url": u})).collect::<Vec<_>>(),
            })
        })
        .collect();
    // Recorded reality (KVG 1996-01-01): an old consolidation can
    // carry its language expressions and NO manifestation at all —
    // not even a PDF. Three honest states, not two.
    let pdf_only = !any_xml && manifestations_total > 0;
    let none_listed = manifestations_total == 0;
    Ok(json!({
        "eli_version": version.eli_version,
        "languages": languages,
        "xml_available": any_xml,
        "pdf_only": pdf_only,
        "no_manifestation_listed": none_listed,
        "manifestations_total": manifestations_total,
        "truncated": truncated,
        "note": if any_xml {
            "XML manifestations exist for the languages marked xml_available; the text tools read those"
        } else if pdf_only {
            "no XML manifestation in any language — the text tools answer not-found for this version; \
             it is readable only at the non-XML URLs listed"
        } else {
            "the graph lists no manifestation file for this version in any language — the text \
             tools answer not-found; the version exists as metadata only (older consolidations)"
        },
        "kind": "norm",
        "provenance": provenance(ctx, &version.date)
    }))
}

const VOCABULARY_BASE: &str = fedlex_jolux::VOCABULARY_BASE;

/// `fedlex.resolve_vocabulary_label` — a Fedlex vocabulary term by
/// label OR by IRI (vendored JLX-VOC-02 label search inside a scheme
/// such as `enforcement-status`, `subdivision-type`, `legal-taxonomy`
/// — and JLX-VOC-01 for an IRI given as the query). `language` is
/// answered from the vendored language table without a query (the
/// language IRIs live at publications.europa.eu). A lookup is a HINT.
pub fn resolve_vocabulary_label(
    ctx: &Ctx,
    vocabulary: &str,
    query: &str,
    lang: Option<&str>,
) -> Result<Value> {
    let scheme = vocabulary.trim();
    if scheme.is_empty()
        || !scheme
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Ok(invalid(
            "vocabulary must be a scheme id like «enforcement-status», «subdivision-type», \
             «legal-taxonomy» or «language»",
        ));
    }
    let q = query.trim();
    if q.is_empty() {
        return Ok(invalid(
            "query must be a label fragment or a vocabulary IRI",
        ));
    }
    let language = match label_lang(lang) {
        Ok(l) => l,
        Err(refusal) => return Ok(refusal),
    };
    if scheme == "language" {
        let all = [
            Language::De,
            Language::Fr,
            Language::It,
            Language::En,
            Language::Roh,
        ];
        let needle = q.to_ascii_lowercase();
        let matches: Vec<Value> = all
            .iter()
            .filter(|l| {
                l.tag() == needle
                    || l.vocab_uri().eq_ignore_ascii_case(q)
                    || l.vocab_uri()
                        .rsplit('/')
                        .next()
                        .is_some_and(|code| code.eq_ignore_ascii_case(q))
            })
            .map(|l| json!({"iri": l.vocab_uri(), "label": l.tag()}))
            .collect();
        if matches.is_empty() {
            return Ok(not_found(&format!("language:{q}")));
        }
        return Ok(json!({
            "vocabulary": "language", "query": q, "matches": matches, "total": matches.len(),
            "truncated": false,
            "source_note": "answered from the vendored official-language table (EU language authority IRIs), no query",
            "kind": "hint", "provenance": provenance(ctx, &ctx.today)
        }));
    }
    let key = format!("resolve_vocabulary_label:{scheme}:{q}:{}", language.tag());
    let client = KeyedClient::new(&ctx.backend, key);
    if q.starts_with(VOCABULARY_BASE) {
        if q.chars()
            .any(|c| c.is_whitespace() || matches!(c, '<' | '>' | '"'))
        {
            return Ok(invalid(
                "query IRI carries characters that cannot form an IRI",
            ));
        }
        if !q.starts_with(&format!("{VOCABULARY_BASE}{scheme}/")) {
            return Ok(invalid(&format!(
                "the IRI «{q}» does not belong to the vocabulary «{scheme}»"
            )));
        }
        // J5.4: the vendored resolver reads the label in ONE language
        // and answers not-found without it. Here every prefLabel of the
        // concept is read in one query and the fallback decides —
        // `answered_in` names the language that actually answered, and
        // `labels` shows what the graph has.
        let found = match vocabulary_labels(ctx, q) {
            Ok(found) => found,
            Err(refusal) => return Ok(refusal),
        };
        let Some((label, answered_in)) = choose_label(&found, Some(language.tag())) else {
            return Ok(not_found(&format!("{scheme}:{q}")));
        };
        let labels: Vec<Value> = found
            .iter()
            .map(|(lang, label)| json!({"lang": lang, "label": label}))
            .collect();
        return Ok(json!({
            "vocabulary": scheme, "query": q, "lang": language.tag(),
            "answered_in": answered_in,
            "matches": [{"iri": q, "label": label, "lang": answered_in}],
            "labels": labels,
            "total": 1, "truncated": false,
            "note": "the label is answered in the language asked for; where the concept carries \
                     none, the fallback de → en → fr → it → rm decides and answered_in names the \
                     language that answered",
            "kind": "hint", "provenance": provenance(ctx, &ctx.today)
        }));
    }
    // One row beyond the cap makes `truncated` a measurement.
    let mut concepts = match run(fedlex_jolux::list_vocabulary(
        &client,
        scheme,
        language,
        MAX_VOCABULARY_MATCHES + 1,
        Some(q),
    )) {
        Ok(c) => c,
        Err(e) => return Ok(jolux_refusal(&e, &format!("{scheme}:{q}"))),
    };
    if concepts.is_empty() {
        return Ok(not_found(&format!("{scheme}:{q}")));
    }
    let truncated = concepts.len() as u32 > MAX_VOCABULARY_MATCHES;
    concepts.truncate(MAX_VOCABULARY_MATCHES as usize);
    // J5.4: the search itself is language-blind — it matches the labels
    // of EVERY language — but the label it hands back is the one in the
    // language asked for, and a concept the graph labels only in French
    // («legal-subject-theme-fr/22158» is «Code» and nothing else) comes
    // back with no label at all. The missing ones are filled in ONE
    // query over exactly those concepts, and every match says which
    // language its label is in.
    let missing: Vec<&str> = concepts
        .iter()
        .filter(|c| c.label.is_none())
        .map(|c| c.uri.as_str())
        .collect();
    let mut filled: std::collections::BTreeMap<String, (String, String)> =
        std::collections::BTreeMap::new();
    if !missing.is_empty() {
        let values = missing
            .iter()
            .map(|uri| format!("<{uri}>"))
            .collect::<Vec<_>>()
            .join(" ");
        let query = format!(
            "{PREFIXES}SELECT ?concept ?label (LANG(?label) AS ?lang) WHERE {{\n\
             VALUES ?concept {{ {values} }}\n\
             ?concept skos:prefLabel ?label .\n\
             }} LIMIT 200"
        );
        let key = format!("vocabulary_labels:missing:{scheme}:{q}:{}", language.tag());
        let value = match ctx.backend.select(&key, &query) {
            Ok(v) => v,
            Err(e) => return Ok(backend_refusal(&e)),
        };
        let mut per_concept: std::collections::BTreeMap<String, Vec<(String, String)>> =
            std::collections::BTreeMap::new();
        for b in bindings_or_refuse!(value) {
            let (Some(concept), Some(label)) = (binding_str(b, "concept"), binding_str(b, "label"))
            else {
                continue;
            };
            per_concept.entry(concept.to_string()).or_default().push((
                binding_str(b, "lang").unwrap_or("").to_ascii_lowercase(),
                label.to_string(),
            ));
        }
        for (uri, labels) in per_concept {
            if let Some(choice) = choose_label(&labels, None) {
                filled.insert(uri, choice);
            }
        }
    }
    let matches: Vec<Value> = concepts
        .iter()
        .map(|c| match (&c.label, filled.get(&c.uri)) {
            (Some(label), _) => {
                json!({"iri": c.uri, "label": label, "label_lang": language.tag()})
            }
            (None, Some((label, lang))) => {
                json!({"iri": c.uri, "label": label, "label_lang": lang})
            }
            (None, None) => json!({"iri": c.uri, "label": Value::Null, "label_lang": Value::Null}),
        })
        .collect();
    Ok(json!({
        "vocabulary": scheme, "query": q, "lang": language.tag(),
        "matches": matches, "returned": matches.len(), "limit": MAX_VOCABULARY_MATCHES,
        "truncated": truncated,
        "labels_filled": filled.len(),
        "note": "the search matches the labels of EVERY language; the label returned is the one \
                 in the language asked for, and where the concept carries none the fallback \
                 de → en → fr → it → rm decides — label_lang says which language each label is in",
        "kind": "hint", "provenance": provenance(ctx, &ctx.today)
    }))
}

/// `fedlex.find_related_topic` — acts in the same field of law, found
/// deterministically over the legal taxonomy (siblings under the same
/// `skos:broader`, vendored JLX-TAX-02). Entry by ELI or by SR number
/// (resolved with the v0 disambiguation). Candidates only: a HINT.
pub fn find_related_topic(
    ctx: &Ctx,
    eli: Option<&str>,
    sr: Option<&str>,
    limit: Option<u32>,
) -> Result<Value> {
    let limit = limit.unwrap_or(20).clamp(1, MAX_RELATED);
    let resolved_eli: String = match (eli, sr) {
        (Some(e), _) => match iri_safe(e) {
            Ok(e) => e.to_string(),
            Err(e) => return Ok(invalid(&format!("{e:#}"))),
        },
        (None, Some(sr)) => {
            let profile = resolve_sr(ctx, sr)?;
            if profile.get("error").is_some() {
                return Ok(profile);
            }
            profile["eli"]
                .as_str()
                .map(str::to_string)
                .unwrap_or_default()
        }
        (None, None) => return Ok(invalid("give eli or sr")),
    };
    let rel = match relative_eli(&resolved_eli) {
        Ok(r) => r,
        Err(refusal) => return Ok(refusal),
    };
    let client = KeyedClient::new(
        &ctx.backend,
        format!("find_related_topic:{resolved_eli}:{limit}"),
    );
    // One row beyond the cap makes `truncated` a measurement, not a
    // guess — the graph does not count for us.
    let mut related = match run(fedlex_jolux::find_related_by_topic(
        &client,
        &rel,
        limit + 1,
    )) {
        Ok(r) => r,
        Err(e) => return Ok(jolux_refusal(&e, &resolved_eli)),
    };
    let truncated = related.len() as u32 > limit;
    related.truncate(limit as usize);
    let hits: Vec<Value> = related
        .iter()
        .map(|r| json!({"eli": full_iri(&r.eli), "sr": r.sr_number}))
        .collect();
    // No `total`: the graph does not count for us, so the honest
    // fields are what came back, the cap, and whether the cap cut.
    Ok(json!({
        "eli": resolved_eli,
        "hits": hits,
        "returned": hits.len(),
        "limit": limit,
        "truncated": truncated,
        "coverage": "siblings under the same taxonomy parent; 85.4 % of the consolidation abstracts \
                     carry a taxonomy entry (vendored fedlex-jolux J20.3)",
        "kind": "hint",
        "provenance": provenance(ctx, &ctx.today)
    }))
}

// ---------------------------------------------------------------------
// BR wave 2, A: research-critical tools (XML + graph)
// ---------------------------------------------------------------------

const MAX_TABLES: usize = 50;
const MAX_TABLE_ROWS: usize = 200;
const MAX_PARSED_REFERENCES: usize = 20;
const MAX_DIFF_ELEMENTS: usize = 200;
const MAX_DIFF_CHARS: usize = 1500;
const MAX_FOREIGN_SECTIONS: usize = 200;
const MAX_NODE_EDGES: u32 = 50;
const MAX_TREATY_HITS: u32 = 50;
const MAX_MEMORIAL_ACTS: u32 = 50;
const MAX_CONSULTATION_DRAFTS: usize = 5;
const MAX_CONSULTATION_DOCUMENTS: usize = 200;

/// Resolves a caller's eId to the document's own spelling, or the
/// typed not-found.
fn scope_eid(
    loaded: &Loaded,
    eli_version: &str,
    eid: Option<&str>,
) -> std::result::Result<Option<String>, Value> {
    match eid {
        None => Ok(None),
        Some(e) => {
            valid_eid(e)?;
            match fedlex_akn::resolve_eid(&loaded.doc, e) {
                Ok(hit) => Ok(Some(loaded.doc.eid(hit.node).unwrap_or(e).to_string())),
                Err(_) => Err(not_found(&format!("{eli_version}#{e}"))),
            }
        }
    }
}

/// Caps a text for a diff side: the first `MAX_DIFF_CHARS` characters
/// and whether that cut.
fn capped_text(text: &str) -> (String, bool) {
    let total = text.chars().count();
    if total <= MAX_DIFF_CHARS {
        (text.to_string(), false)
    } else {
        (text.chars().take(MAX_DIFF_CHARS).collect(), true)
    }
}

/// `fedlex.extract_tables` — the tables of a consolidation (or of one
/// element of it: an annex with limit values, a tariff) as rows and
/// columns with a header row (vendored AKN-SPC-01). Fedlex tables
/// rarely mark their header with `<th>`; when no header row is
/// marked, the FIRST row is taken as the header and said so
/// (`header_inferred`). Capped: at most 50 tables, 200 rows each, with
/// the original sizes.
pub fn extract_tables(
    ctx: &Ctx,
    eli_version: &str,
    eid: Option<&str>,
    lang: Option<&str>,
) -> Result<Value> {
    let loaded = loaded_or_refuse!(load_version(ctx, eli_version, lang));
    let scope = match scope_eid(&loaded, eli_version, eid) {
        Ok(s) => s,
        Err(refusal) => return Ok(refusal),
    };
    // X6.3: the vendored extractor keeps <th> cells only as the header
    // and only in row 0 — a row of <th> cells further down (a SECOND
    // header, as tables with two-level column groups write it) was
    // dropped with everything in it. The rows are walked here.
    let scope_node = match scope.as_deref() {
        Some(eid) => match fedlex_akn::resolve_eid(&loaded.doc, eid) {
            Ok(hit) => hit.node,
            Err(e) => return Ok(element_not_found(eli_version, eid, &e)),
        },
        None => loaded.doc.root(),
    };
    let table_nodes = loaded.doc.find_all(scope_node, "table");
    let total = table_nodes.len();
    let out: Vec<Value> = table_nodes
        .iter()
        .take(MAX_TABLES)
        .map(|node| table_json(&loaded.doc, *node))
        .collect();
    Ok(json!({
        "eli_version": eli_version,
        "lang": loaded.lang,
        "eid": scope,
        "tables": out,
        "total": total,
        "returned": out.len(),
        "truncated": out.len() < total,
        "kind": "norm",
        "provenance": provenance_served(ctx, &loaded)
    }))
}

/// One table as the answer carries it (X6.3). Row 0 is the header —
/// its `<th>` cells where it has them, else its `<td>` cells with
/// `header_inferred` saying so (Fedlex marks no `<th>` in the recorded
/// corpus). Every later row is a data row: `<td>` cells where it has
/// them, and a row of `<th>` cells only is KEPT as a data row, because
/// its cells carry the column meaning the numbers below need — its
/// index in `data` is named in `sub_header_rows`, so a reader can tell
/// it from a measurement.
fn table_json(doc: &AknDocument, node: fedlex_akn::NodeId) -> Value {
    let trs = doc.find_all(node, "tr");
    let mut header: Vec<String> = Vec::new();
    let mut header_inferred = false;
    let mut data: Vec<Vec<String>> = Vec::new();
    let mut sub_header_rows: Vec<usize> = Vec::new();
    let mut cols = 0usize;
    for (index, &tr) in trs.iter().enumerate() {
        let cells = |tag: &str| -> Vec<String> {
            doc.children(tr)
                .filter(|&c| doc.tag(c) == tag)
                .map(|c| doc.text_of(c).trim().to_string())
                .collect()
        };
        let header_cells = cells("th");
        let data_cells = cells("td");
        cols = cols.max(header_cells.len() + data_cells.len());
        if index == 0 {
            if !header_cells.is_empty() {
                header = header_cells;
                continue;
            }
            if !data_cells.is_empty() {
                header = data_cells;
                header_inferred = true;
                continue;
            }
            continue;
        }
        if !data_cells.is_empty() {
            data.push(data_cells);
        } else if !header_cells.is_empty() {
            sub_header_rows.push(data.len());
            data.push(header_cells);
        }
    }
    let rows_total = data.len();
    let rows: Vec<&Vec<String>> = data.iter().take(MAX_TABLE_ROWS).collect();
    let sub_header_rows: Vec<usize> = sub_header_rows
        .into_iter()
        .filter(|i| *i < rows.len())
        .collect();
    json!({
        "context_eid": doc.nearest_eid(node),
        "rows": trs.len(),
        "cols": cols,
        "header": header,
        "header_inferred": header_inferred,
        "data": rows,
        "rows_total": rows_total,
        "rows_returned": rows.len(),
        "sub_header_rows": sub_header_rows,
        "truncated": rows.len() < rows_total,
        "oversized": trs.len() > 100,
    })
}

// --- parse_reference: a legal citation in plain text → addresses ----

/// One parsed reference (see [`parse_reference`]).
#[derive(Default, Debug)]
struct ParsedReference {
    raw: String,
    kind: &'static str,
    abbreviation: Option<String>,
    article: Option<String>,
    paragraph: Option<String>,
    letter: Option<String>,
    number: Option<String>,
    annex: Option<String>,
    following: bool,
    sr: Option<String>,
    memorial: Option<String>,
}

/// Strips trailing punctuation a token may carry («25,», «b)», «3.»).
fn clean_token(token: &str) -> String {
    token
        .trim_matches(|c: char| matches!(c, ',' | ';' | ')' | '(' | ':'))
        .trim_end_matches('.')
        .to_string()
}

/// An article number: digits, then optional letters (`25`, `23a`, `9bis`).
fn is_article_number(s: &str) -> bool {
    let mut chars = s.chars();
    chars.next().is_some_and(|c| c.is_ascii_digit()) && s.chars().all(|c| c.is_ascii_alphanumeric())
}

/// The citation vocabulary — ONE place for both directions (BT).
/// `parse_reference` reads any of these spellings (the reading
/// direction), `cite` writes the first one of each list, the canonical
/// form per language (the writing direction). A spelling added here
/// is read and written consistently, which is the point of one table.
struct CitationVocabulary {
    lang: &'static str,
    article: &'static [&'static str],
    paragraph: &'static [&'static str],
    letter: &'static [&'static str],
    number: &'static [&'static str],
    annex: &'static [&'static str],
}

/// The first entry of every list is the canonical written form. German
/// letters are «Bst.», not «lit.» (BT′, from the audit): the Fedlex
/// texts themselves write «Bst.» — the recorded KVG carries it 7×, the
/// LSV 1×, «lit.» 0× — and so do the Federal Chancellery's
/// Gesetzestechnische Richtlinien; «lit.» is the spelling of doctrine
/// and the courts, read but not written.
const VOCABULARY: [CitationVocabulary; 3] = [
    CitationVocabulary {
        lang: "de",
        article: &["Art.", "Artikel", "Arts.", "Artt."],
        paragraph: &["Abs.", "Absatz", "Absätze"],
        letter: &["Bst.", "lit.", "Buchstabe"],
        number: &["Ziff.", "Ziffer"],
        annex: &["Anhang", "Anh.", "Annex"],
    },
    CitationVocabulary {
        lang: "fr",
        article: &["art.", "article"],
        paragraph: &["al.", "alinéa"],
        letter: &["let.", "lettre"],
        number: &["ch.", "chiffre"],
        annex: &["annexe"],
    },
    CitationVocabulary {
        lang: "it",
        article: &["art.", "articolo"],
        paragraph: &["cpv.", "capoverso"],
        letter: &["lett.", "lettera"],
        number: &["n.", "numero"],
        annex: &["allegato"],
    },
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum CitationPart {
    Article,
    Paragraph,
    Letter,
    Number,
    Annex,
}

/// Which part of a citation a token names, in any language — the
/// reading direction. Tokens arrive cleaned of trailing punctuation
/// («Abs.» → «Abs»); the match is case-insensitive.
fn citation_part(token: &str) -> Option<CitationPart> {
    let t = token.trim_end_matches('.').to_lowercase();
    if t.is_empty() {
        return None;
    }
    let names = |words: &[&str]| {
        words
            .iter()
            .any(|w| w.trim_end_matches('.').to_lowercase() == t)
    };
    for v in &VOCABULARY {
        if names(v.article) {
            return Some(CitationPart::Article);
        }
        if names(v.paragraph) {
            return Some(CitationPart::Paragraph);
        }
        if names(v.letter) {
            return Some(CitationPart::Letter);
        }
        if names(v.number) {
            return Some(CitationPart::Number);
        }
        if names(v.annex) {
            return Some(CitationPart::Annex);
        }
    }
    None
}

/// The canonical written form of a part in a language — the writing
/// direction (`cite`). An unknown language writes German.
fn citation_word(lang: &str, part: CitationPart) -> &'static str {
    let v = VOCABULARY
        .iter()
        .find(|v| v.lang == lang)
        .unwrap_or(&VOCABULARY[0]);
    match part {
        CitationPart::Article => v.article[0],
        CitationPart::Paragraph => v.paragraph[0],
        CitationPart::Letter => v.letter[0],
        CitationPart::Number => v.number[0],
        CitationPart::Annex => v.annex[0],
    }
}

/// An official abbreviation token: an upper-case start, letters and
/// digits, possibly a German umlaut (BGÖ, StPO, ArGV, BV, EMRK) —
/// never a citation word or a memorial keyword.
fn is_abbreviation_token(s: &str) -> bool {
    let mut chars = s.chars();
    chars.next().is_some_and(char::is_uppercase)
        && s.chars().all(|c| c.is_alphanumeric())
        && s.chars().count() <= 12
        && citation_part(s).is_none()
        && !matches!(s, "SR" | "AS" | "BBl")
}

/// The Latin suffixes an inserted letter or paragraph carries
/// («Bst. fbis», «Abs. 1bis»); the manifestations write them as
/// `lbl_f_bis`, `para_1_bis`.
const LATIN_SUFFIXES: [&str; 9] = [
    "bis",
    "ter",
    "quater",
    "quinquies",
    "sexies",
    "septies",
    "octies",
    "novies",
    "decies",
];

/// Splits «fbis» into («f», Some(«bis»)), «b» into («b», None).
fn split_latin_suffix(token: &str) -> (&str, Option<&str>) {
    for suffix in LATIN_SUFFIXES {
        if let Some(base) = token.strip_suffix(suffix) {
            if !base.is_empty() {
                return (base, Some(suffix));
            }
        }
    }
    (token, None)
}

/// A letter token of a citation: up to three Latin letters («b»,
/// «aa»), or up to two followed by a Latin suffix («fbis», «gter»).
fn is_letter_token(token: &str) -> bool {
    // A bare Latin suffix is not a letter — «Bst. bis» names nothing (BT′).
    if LATIN_SUFFIXES.contains(&token) {
        return false;
    }
    let (base, suffix) = split_latin_suffix(token);
    let max = if suffix.is_some() { 2 } else { 3 };
    !base.is_empty() && base.len() <= max && base.chars().all(|c| c.is_ascii_alphabetic())
}

/// The AKN eId path for a parsed article reference, in the spelling
/// the Fedlex manifestations use (`art_25_a`, `para_1_bis`,
/// `lbl_b`, `lbl_f_bis`, `lbl_2` for a Ziff. below a letter —
/// recorded conventions of the KVG and BGÖ manifestations).
fn eid_candidate(r: &ParsedReference) -> Option<String> {
    let article = r.article.as_deref()?;
    let digits: String = article.chars().take_while(|c| c.is_ascii_digit()).collect();
    let suffix: String = article
        .chars()
        .skip_while(|c| c.is_ascii_digit())
        .collect::<String>()
        .to_lowercase();
    let mut path = if suffix.is_empty() {
        format!("art_{digits}")
    } else {
        format!("art_{digits}_{suffix}")
    };
    if let Some(p) = &r.paragraph {
        let pd: String = p.chars().take_while(|c| c.is_ascii_digit()).collect();
        let ps: String = p
            .chars()
            .skip_while(|c| c.is_ascii_digit())
            .collect::<String>()
            .to_lowercase();
        path.push_str(&format!("/para_{pd}"));
        if !ps.is_empty() {
            path.push_str(&format!("_{ps}"));
        }
    }
    if let Some(l) = &r.letter {
        let lower = l.to_lowercase();
        let (base, suffix) = split_latin_suffix(&lower);
        path.push_str(&format!("/lbl_{base}"));
        if let Some(suffix) = suffix {
            path.push_str(&format!("_{suffix}"));
        }
    }
    if let Some(n) = &r.number {
        if r.paragraph.is_some() || r.letter.is_some() {
            path.push_str(&format!("/lbl_{n}"));
        }
    }
    Some(path)
}

/// Parses ONE reference segment («Art. 7 Abs. 1 lit. b LSV»).
fn parse_segment(segment: &str) -> ParsedReference {
    let mut r = ParsedReference {
        raw: segment.trim().to_string(),
        kind: "unknown",
        ..Default::default()
    };
    let tokens: Vec<String> = segment.split_whitespace().map(clean_token).collect();
    let mut i = 0;
    let mut trailing: Vec<String> = Vec::new();
    while i < tokens.len() {
        let t = tokens[i].as_str();
        let next = tokens.get(i + 1).map(String::as_str);
        match citation_part(t) {
            Some(CitationPart::Article) => {
                if let Some(n) = next.filter(|n| is_article_number(n)) {
                    r.article = Some(n.to_string());
                    r.kind = "article";
                    i += 2;
                    continue;
                }
            }
            Some(CitationPart::Paragraph) => {
                if let Some(n) = next.filter(|n| is_article_number(n)) {
                    r.paragraph = Some(n.to_string());
                    i += 2;
                    continue;
                }
            }
            Some(CitationPart::Letter) => {
                if let Some(l) = next.filter(|l| is_letter_token(l)) {
                    r.letter = Some(l.to_string());
                    i += 2;
                    continue;
                }
            }
            Some(CitationPart::Number) => {
                if let Some(n) = next.filter(|n| is_article_number(n)) {
                    r.number = Some(n.to_string());
                    i += 2;
                    continue;
                }
            }
            Some(CitationPart::Annex) => {
                if let Some(n) = next.filter(|n| is_article_number(n)) {
                    r.annex = Some(n.to_string());
                    if r.kind == "unknown" {
                        r.kind = "annex";
                    }
                    i += 2;
                    continue;
                }
            }
            None => {}
        }
        match t {
            "ff" | "f" => {
                r.following = true;
                i += 1;
                continue;
            }
            "SR" | "RS" => {
                if let Some(n) = next.filter(|n| {
                    n.chars().all(|c| c.is_ascii_digit() || c == '.')
                        && n.contains(|c: char| c.is_ascii_digit())
                }) {
                    r.sr = Some(n.to_string());
                    if r.kind == "unknown" {
                        r.kind = "sr";
                    }
                    i += 2;
                    continue;
                }
            }
            "AS" | "RO" | "RU" | "BBl" | "FF" => {
                let rest: Vec<&str> = tokens[i + 1..]
                    .iter()
                    .map(String::as_str)
                    .take_while(|x| x.chars().all(|c| c.is_ascii_digit()))
                    .collect();
                if !rest.is_empty() {
                    r.memorial = Some(format!("{t} {}", rest.join(" ")));
                    if r.kind == "unknown" {
                        r.kind = if t == "BBl" || t == "FF" { "bbl" } else { "as" };
                    }
                    i += 1 + rest.len();
                    continue;
                }
            }
            _ => {}
        }
        if is_abbreviation_token(t) {
            trailing.push(t.to_string());
        } else if !trailing.is_empty() && t.chars().all(|c| c.is_ascii_digit()) && t.len() <= 2 {
            // «ArGV 1», «VBO 2» — a numbered ordinance abbreviation.
            let last = trailing.pop().unwrap_or_default();
            trailing.push(format!("{last} {t}"));
        }
        i += 1;
    }
    // The abbreviation is the LAST abbreviation-shaped token — the
    // act name closes a Swiss citation («Art. 25 Abs. 1 USG»).
    r.abbreviation = trailing.pop();
    r
}

/// Splits a citation string into its references: «i.V.m.», «in
/// Verbindung mit», «;» and «sowie» separate them.
fn split_references(text: &str) -> Vec<String> {
    let mut normalised = text
        .replace("i. V. m.", " i.V.m. ")
        .replace("i.V.m.", " i.V.m. ");
    normalised = normalised
        .replace("in Verbindung mit", " i.V.m. ")
        .replace(" sowie ", " ; ")
        .replace(" und Art.", " ; Art.")
        .replace(" und Anhang", " ; Anhang");
    normalised
        .split(';')
        .flat_map(|part| part.split("i.V.m."))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// The abbreviation pre-query of `search_law`, shared with
/// `parse_reference`: exact, case-insensitive on `jolux:titleShort`.
/// Fixture key `search_law:abbreviation:<query>`.
fn abbreviation_hits(
    ctx: &Ctx,
    q: &str,
) -> Result<std::result::Result<Vec<(String, SearchHit)>, Value>> {
    let escaped = sparql_escape(&q.to_lowercase())?;
    let query = format!(
        "{PREFIXES}SELECT ?ca ?lang ?title ?short ?status ?sr WHERE {{\n\
         ?ca a jolux:ConsolidationAbstract ; jolux:isRealizedBy ?e .\n\
         ?e jolux:language ?lang ; jolux:title ?title ; jolux:titleShort ?short .\n\
         FILTER(LCASE(STR(?short)) = \"{escaped}\")\n\
         OPTIONAL {{ ?ca jolux:inForceStatus ?status }}\n\
         OPTIONAL {{ ?ca jolux:classifiedByTaxonomyEntry ?t . ?t skos:notation ?sr }}\n\
         }} LIMIT 60"
    );
    let value = match ctx
        .backend
        .select(&format!("search_law:abbreviation:{q}"), &query)
    {
        Ok(v) => v,
        Err(e) => return Ok(Err(backend_refusal(&e))),
    };
    let rows = match Backend::bindings(&value) {
        Ok(b) => b,
        Err(e) => return Ok(Err(upstream(format!("{e:#}")))),
    };
    let mut folded = Vec::new();
    fold_search_rows(rows, 0, &mut folded);
    folded.sort_by(|(eli_a, a), (eli_b, b)| {
        let rank = |eli: &str, h: &SearchHit| {
            (
                if h.status.as_deref() == Some(IN_FORCE_STATUS) {
                    0
                } else {
                    1
                },
                sr_rank(h.sr.as_deref()),
                std::cmp::Reverse(eli_year(eli)),
            )
        };
        rank(eli_a, a).cmp(&rank(eli_b, b))
    });
    Ok(Ok(folded))
}

fn act_json(eli: &str, hit: &SearchHit, today: &str) -> Value {
    json!({
        "eli": eli,
        "sr": hit.sr,
        "title": preferred(&hit.titles).map(|(_, v)| v.clone()),
        // The shared rule (J3.1/J3.2), as everywhere else.
        "in_force": in_force_at(today, None, None, None, hit.status.as_deref()),
        "status": hit.status,
    })
}

/// `fedlex.parse_reference` — a legal citation in plain text («Art. 25
/// Abs. 1 USG», «Art. 7 Abs. 1 lit. b LSV», «Anhang 3 Ziff. 2 LSV»,
/// «Art. 8 EMRK i.V.m. Art. 36 BV», «Art. 41 ff. OR», «SR 832.10»,
/// «AS 2020 752») taken apart into readable addresses: the act by its
/// official abbreviation (the BO′ pre-query on `jolux:titleShort`;
/// `unresolved: true` when the graph knows no such abbreviation), the
/// article eId, the paragraph / letter / number as a path proposal in
/// the manifestations' own spelling (`art_7/para_1/lbl_b`,
/// `art_25_a/para_1_bis`), an annex prefix with a hint. A HINT: the
/// address becomes a norm only when `read_article` returns it. The
/// tool that turns a model's or a marker's citation into something a
/// machine can open — the ground of the citation chain.
pub fn parse_reference(ctx: &Ctx, text: &str) -> Result<Value> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(invalid("text must be a citation like «Art. 25 Abs. 1 USG»"));
    }
    if text.chars().count() > 1000 {
        return Ok(invalid(
            "text must be a citation, not a document (at most 1000 characters)",
        ));
    }
    let segments = split_references(text);
    let total = segments.len();
    let mut references = Vec::new();
    let mut acts: std::collections::BTreeMap<String, Value> = std::collections::BTreeMap::new();
    for segment in segments.iter().take(MAX_PARSED_REFERENCES) {
        let parsed = parse_segment(segment);
        // The act: by abbreviation (shared pre-query) or by SR number.
        let (act, unresolved) = if let Some(abbr) = &parsed.abbreviation {
            if let Some(cached) = acts.get(abbr) {
                (cached.clone(), cached.is_null())
            } else {
                let act = match abbreviation_hits(ctx, abbr)? {
                    Ok(hits) => hits
                        .first()
                        .map(|(eli, hit)| act_json(eli, hit, &ctx.today))
                        .unwrap_or(Value::Null),
                    Err(refusal) => return Ok(refusal),
                };
                acts.insert(abbr.clone(), act.clone());
                let unresolved = act.is_null();
                (act, unresolved)
            }
        } else if let Some(sr) = &parsed.sr {
            let profile = resolve_sr(ctx, sr)?;
            if profile["error"] == "not-found" {
                (Value::Null, true)
            } else if profile.get("error").is_some() {
                return Ok(profile);
            } else {
                (
                    // J5.3/J3.1: the act's profile has already decided
                    // in_force by the act's own dates and has read the
                    // status label in the first language the catalogue
                    // carries — the citation answer repeats that
                    // decision instead of comparing the status IRI.
                    json!({
                        "eli": profile["eli"], "sr": sr,
                        "title": profile["title"]["de"].clone(),
                        "in_force": profile["in_force"],
                        "status": profile["status"],
                        "status_label": profile["status_label"],
                        "status_unset": profile["status_unset"],
                    }),
                    false,
                )
            }
        } else {
            (Value::Null, false)
        };
        let eid = eid_candidate(&parsed);
        let annex_hint = parsed.annex.as_ref().map(|n| {
            format!(
                "annex_{n}: annex levels carry generic eIds (annex_{n}/lvl_u1/…) — locate the \
                 place with fedlex.list_annexes and fedlex.get_structure or fedlex.search_text, \
                 then read it with fedlex.read_article{}",
                parsed
                    .number
                    .as_ref()
                    .map(|z| format!("; the Ziff. {z} is a heading inside that annex"))
                    .unwrap_or_default()
            )
        });
        let next = match (&act, &eid) {
            (Value::Null, _) => "resolve the act first (fedlex.search_law by title or fedlex.resolve_sr), then fedlex.resolve_consolidation_at → fedlex.read_article",
            (_, Some(_)) => "fedlex.resolve_consolidation_at(act.eli, as_of) → fedlex.read_article(eli_version, eid_candidate)",
            (_, None) => "fedlex.resolve_consolidation_at(act.eli, as_of) → fedlex.get_structure / fedlex.list_annexes",
        };
        references.push(json!({
            "raw": parsed.raw,
            "kind": parsed.kind,
            "abbreviation": parsed.abbreviation,
            "act": act,
            "unresolved": unresolved,
            "article": parsed.article,
            "paragraph": parsed.paragraph,
            "letter": parsed.letter,
            "number": parsed.number,
            "annex": parsed.annex,
            "following": parsed.following,
            "sr": parsed.sr,
            "memorial": parsed.memorial,
            "eid_candidate": eid,
            "annex_hint": annex_hint,
            "next": next,
        }));
    }
    Ok(json!({
        "text": text,
        "references": references,
        "total": total,
        "returned": references.len(),
        "truncated": references.len() < total,
        "kind": "hint",
        "provenance": provenance(ctx, &ctx.today)
    }))
}

/// `fedlex.get_citations`, the formal citation graph (vendored
/// JLX-CIT-01 over the bridge): what the act's text formally cites
/// (`cites`, the version governing today) and who formally cites it
/// (`cited_by`, in any recorded version). Text level only — JOLux
/// carries no article-level citations (J7.1); the overlap with the
/// in-text refs of `get_references` is 0–48 % (J7.3), so a complete
/// picture merges both.
fn formal_citations(ctx: &Ctx, eli: &str, direction: &str) -> Result<Value> {
    let rel = match relative_eli(eli) {
        Ok(r) => r,
        Err(refusal) => return Ok(refusal),
    };
    let as_of = match valid_as_of(&ctx.today) {
        Ok(d) => d,
        Err(refusal) => return Ok(refusal),
    };
    let which = match direction {
        "cites" => fedlex_jolux::CitationDirection::Outgoing,
        _ => fedlex_jolux::CitationDirection::Incoming,
    };
    let client = KeyedClient::new(&ctx.backend, format!("get_citations:{eli}:{direction}"));
    let citations = match run(fedlex_jolux::get_citations(&client, &rel, which, as_of)) {
        Ok(response) => response.into_parts().0,
        Err(e) => return Ok(jolux_refusal(&e, eli)),
    };
    let list: Vec<Value> = citations
        .iter()
        .map(|c| json!({"from": c.from, "to": c.to, "description": c.description}))
        .collect();
    Ok(json!({
        "citations": list,
        "total": list.len(),
        "direction": direction,
        "coverage": "formal citation graph (JOLux), act level only — no article-level citations exist there; \
                     merge with fedlex.get_references (in-text refs) for the full picture. cites = the \
                     version governing today; cited_by = cited in any recorded version",
        "kind": "norm",
        "provenance": provenance(ctx, &ctx.today)
    }))
}

// --- compare_versions --------------------------------------------------

/// A version argument: a full version IRI, or a date (YYYYMMDD /
/// YYYY-MM-DD) joined with the act's ELI.
fn version_iri(eli: &str, version: &str) -> std::result::Result<String, Value> {
    let v = version.trim();
    if v.starts_with("https://") {
        return Ok(v.to_string());
    }
    let digits: String = v.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() == 8 && v.chars().all(|c| c.is_ascii_digit() || c == '-') {
        Ok(format!("{eli}/{digits}"))
    } else {
        Err(invalid(&format!(
            "version «{v}» must be a version IRI (<eli>/YYYYMMDD) or a date YYYY-MM-DD"
        )))
    }
}

/// The comparable units of an element: its eId-bearing hierarchy
/// children (the paragraphs of an article) — or the element itself
/// when it has none.
fn units_of(doc: &AknDocument, eid: &str) -> Vec<(String, String)> {
    let Some(&node) = doc.lookup_eid(eid).first() else {
        return Vec::new();
    };
    let children: Vec<(String, String)> = doc
        .children(node)
        .flat_map(|c| {
            // Paragraphs may sit under a <content> wrapper: look one level down too.
            if fedlex_akn::is_hierarchy_tag(doc.tag(c)) {
                vec![c]
            } else {
                doc.children(c)
                    .filter(|&g| fedlex_akn::is_hierarchy_tag(doc.tag(g)))
                    .collect()
            }
        })
        .filter_map(|c| doc.eid(c).map(|e| (e.to_string(), doc.text_of(c))))
        .collect();
    if children.is_empty() {
        vec![(eid.to_string(), doc.text_of(node))]
    } else {
        children
    }
}

/// The article eIds of a document in document order (flat), or the
/// one scoped element.
fn compared_elements(doc: &AknDocument, as_of: ValidAsOf, scope: Option<&str>) -> Vec<String> {
    if let Some(e) = scope {
        return vec![e.to_string()];
    }
    fedlex_akn::get_document_structure(doc, Some("article"), as_of)
        .map(|r| r.into_parts().0.into_iter().filter_map(|n| n.eid).collect())
        .unwrap_or_default()
}

/// `fedlex.compare_versions` — what changed in an element (or in every
/// article) between two consolidations of the same act: added,
/// removed and changed elements, and for a changed one its paragraphs
/// with the wording before and after (each side capped at 1500
/// characters, said so). Both manifestations go through the cache.
pub fn compare_versions(
    ctx: &Ctx,
    eli: &str,
    from_version: &str,
    to_version: &str,
    eid: Option<&str>,
    lang: Option<&str>,
) -> Result<Value> {
    let eli = match iri_safe(eli) {
        Ok(e) => e,
        Err(e) => return Ok(invalid(&format!("{e:#}"))),
    };
    let from_iri = match version_iri(eli, from_version) {
        Ok(v) => v,
        Err(refusal) => return Ok(refusal),
    };
    let to_iri = match version_iri(eli, to_version) {
        Ok(v) => v,
        Err(refusal) => return Ok(refusal),
    };
    if !from_iri.starts_with(eli) || !to_iri.starts_with(eli) {
        return Ok(invalid("both versions must belong to the given act"));
    }
    if from_iri == to_iri {
        return Ok(invalid(
            "from_version and to_version are the same consolidation",
        ));
    }
    let from = loaded_or_refuse!(load_version(ctx, &from_iri, lang));
    let to = loaded_or_refuse!(load_version(ctx, &to_iri, lang));
    let scope = match eid {
        Some(e) => {
            if let Err(refusal) = valid_eid(e) {
                return Ok(refusal);
            }
            // Present in at least one side, else not-found.
            let in_from = fedlex_akn::resolve_eid(&from.doc, e).ok();
            let in_to = fedlex_akn::resolve_eid(&to.doc, e).ok();
            match (in_from, in_to) {
                (None, None) => return Ok(not_found(&format!("{eli}#{e} in either version"))),
                (_, Some(hit)) => Some(to.doc.eid(hit.node).unwrap_or(e).to_string()),
                (Some(hit), None) => Some(from.doc.eid(hit.node).unwrap_or(e).to_string()),
            }
        }
        None => None,
    };
    let from_elements = compared_elements(&from.doc, from.as_of, scope.as_deref());
    let to_elements = compared_elements(&to.doc, to.as_of, scope.as_deref());
    let mut order: Vec<String> = to_elements.clone();
    for e in &from_elements {
        if !order.contains(e) {
            order.push(e.clone());
        }
    }
    let mut added = Vec::new();
    let mut removed = Vec::new();
    let mut changed = Vec::new();
    let mut unchanged = 0usize;
    for e in &order {
        let in_from = !from.doc.lookup_eid(e).is_empty();
        let in_to = !to.doc.lookup_eid(e).is_empty();
        match (in_from, in_to) {
            (false, true) => added.push(e.clone()),
            (true, false) => removed.push(e.clone()),
            (true, true) => {
                let before = units_of(&from.doc, e);
                let after = units_of(&to.doc, e);
                if before == after {
                    unchanged += 1;
                    continue;
                }
                let mut units = Vec::new();
                let mut seen: Vec<&str> = Vec::new();
                for (ueid, text_after) in &after {
                    seen.push(ueid);
                    match before.iter().find(|(b, _)| b == ueid) {
                        None => {
                            let (t, cut) = capped_text(text_after);
                            units.push(json!({"eid": ueid, "change": "added", "after": t, "after_truncated": cut}));
                        }
                        Some((_, text_before)) if text_before != text_after => {
                            let (b, bc) = capped_text(text_before);
                            let (a, ac) = capped_text(text_after);
                            units.push(json!({"eid": ueid, "change": "changed", "before": b, "before_truncated": bc, "after": a, "after_truncated": ac}));
                        }
                        Some(_) => {}
                    }
                }
                for (ueid, text_before) in &before {
                    if !seen.contains(&ueid.as_str()) {
                        let (t, cut) = capped_text(text_before);
                        units.push(json!({"eid": ueid, "change": "removed", "before": t, "before_truncated": cut}));
                    }
                }
                let node = to.doc.lookup_eid(e).first().copied();
                let (num, heading) = node
                    .map(|n| {
                        (
                            to.doc.find_child(n, "num").map(|x| to.doc.text_of(x)),
                            to.doc.find_child(n, "heading").map(|x| to.doc.text_of(x)),
                        )
                    })
                    .unwrap_or((None, None));
                changed.push(json!({"eid": e, "num": num, "heading": heading, "units": units}));
            }
            (false, false) => {}
        }
    }
    let changes_total = added.len() + removed.len() + changed.len();
    let truncated = changed.len() > MAX_DIFF_ELEMENTS;
    changed.truncate(MAX_DIFF_ELEMENTS);
    Ok(json!({
        "eli": eli,
        "from": {"version": from_iri, "date": from.date, "served": from.served.as_str(),
                 "transaction_time": from.retrieved_at.clone().unwrap_or_else(|| ctx.today.clone())},
        "to": {"version": to_iri, "date": to.date, "served": to.served.as_str(),
               "transaction_time": to.retrieved_at.clone().unwrap_or_else(|| ctx.today.clone())},
        "eid": scope,
        "compared": order.len(),
        "unchanged": unchanged,
        "added": added,
        "removed": removed,
        "changed": changed,
        "changes_total": changes_total,
        "truncated": truncated,
        "granularity": "articles (or the scoped element) compared by their paragraphs' norm text; \
                        footnotes excluded",
        "kind": "norm",
        "provenance": provenance_served(ctx, &to)
    }))
}

/// `fedlex.explore_node` — the predicates and neighbours of any node
/// of the JOLux graph, both directions, each capped (vendored
/// JLX-VOC-03 over the bridge; one row beyond the cap so `truncated`
/// is measured). A debugging tool for agents, honestly a HINT: it
/// shows edges, it proves nothing.
pub fn explore_node(ctx: &Ctx, iri: &str, limit: Option<u32>) -> Result<Value> {
    let iri = iri.trim();
    if !iri.starts_with("https://fedlex.data.admin.ch/")
        || iri
            .chars()
            .any(|c| c.is_whitespace() || matches!(c, '<' | '>' | '"' | '\\'))
    {
        return Ok(invalid(
            "iri must be a Fedlex IRI (https://fedlex.data.admin.ch/…: an ELI, a version, a \
             vocabulary term, an impact or process node)",
        ));
    }
    let limit = limit.unwrap_or(20).clamp(1, MAX_NODE_EDGES);
    let client = KeyedClient::new(&ctx.backend, format!("explore_node:{iri}:{limit}"));
    let hood = match run(fedlex_jolux::explore_node(&client, iri, limit + 1)) {
        Ok(h) => h,
        Err(e) => return Ok(jolux_refusal(&e, iri)),
    };
    let edges = |list: &[fedlex_jolux::NodeEdge]| -> (Vec<Value>, bool) {
        let truncated = list.len() as u32 > limit;
        (
            list.iter()
                .take(limit as usize)
                .map(|e| json!({"predicate": e.predicate, "value": e.value}))
                .collect(),
            truncated,
        )
    };
    let (outgoing, out_truncated) = edges(&hood.outgoing);
    let (incoming, in_truncated) = edges(&hood.incoming);
    if outgoing.is_empty() && incoming.is_empty() {
        return Ok(not_found(iri));
    }
    Ok(json!({
        "iri": iri,
        "outgoing": outgoing,
        "outgoing_truncated": out_truncated,
        "incoming": incoming,
        "incoming_truncated": in_truncated,
        "limit": limit,
        "truncated": out_truncated || in_truncated,
        "note": "a debugging view of the graph: edges as stored, no interpretation — use the typed tools to prove anything",
        "kind": "hint",
        "provenance": provenance(ctx, &ctx.today)
    }))
}

/// Sections whose `xml:lang` differs from the manifestation's language
/// (foreign-language quotations, treaty texts), nested ones folded
/// into their outermost section; the `meta` block is skipped (its
/// FRBR names carry every language by design).
fn foreign_language_sections(doc: &AknDocument, base_lang: &str) -> Vec<Value> {
    let mut out = Vec::new();
    let mut skip: std::collections::HashSet<usize> = std::collections::HashSet::new();
    let root = doc.root();
    for node in doc.descendants(root) {
        if skip.contains(&node) {
            continue;
        }
        let tag = doc.tag(node);
        if tag == "meta" {
            skip.extend(doc.descendants(node));
            continue;
        }
        let Some(lang) = doc.attr(node, "lang") else {
            continue;
        };
        let code = lang.to_ascii_lowercase();
        if code.starts_with(base_lang) {
            continue;
        }
        let text = doc.text_of(node);
        out.push(json!({
            "eid": doc.nearest_eid(node),
            "element_kind": tag,
            "lang": lang,
            "chars": text.chars().count(),
            "snippet": text.chars().take(160).collect::<String>(),
        }));
        skip.extend(doc.descendants(node));
    }
    out
}

/// `fedlex.detect_foreign_content` — what the plain-text tools cannot
/// show as text: sections in another language than the manifestation
/// (an `xml:lang` that deviates — foreign quotations, treaty texts)
/// and the `<foreign>` islands (SVG graphics, MathML formulas, OOXML
/// rests; vendored AKN-SPC-02). A norm about the authentic text's own
/// markup; an empty answer means the manifestation marks nothing.
pub fn detect_foreign_content(ctx: &Ctx, eli_version: &str, lang: Option<&str>) -> Result<Value> {
    let loaded = loaded_or_refuse!(load_version(ctx, eli_version, lang));
    let sections = foreign_language_sections(&loaded.doc, loaded.lang);
    let sections_total = sections.len();
    let sections: Vec<Value> = sections.into_iter().take(MAX_FOREIGN_SECTIONS).collect();
    let islands_all = fedlex_akn::detect_foreign_content(&loaded.doc);
    let islands_total = islands_all.len();
    let islands: Vec<Value> = islands_all
        .iter()
        .take(MAX_FOREIGN_SECTIONS)
        .map(|f| {
            json!({
                "context_eid": f.context_eid,
                "kind": format!("{:?}", f.kind).to_lowercase(),
                "element_count": f.element_count,
            })
        })
        .collect();
    Ok(json!({
        "eli_version": eli_version,
        "lang": loaded.lang,
        "foreign_language_sections": sections,
        "sections_total": sections_total,
        "sections_truncated": sections.len() < sections_total,
        "foreign_islands": islands,
        "islands_total": islands_total,
        "islands_truncated": islands.len() < islands_total,
        "truncated": sections.len() < sections_total || islands.len() < islands_total,
        "note": "sections = elements whose xml:lang deviates from the manifestation's language; \
                 islands = <foreign> markup the text tools exclude (formulas, graphics)",
        "kind": "norm",
        "provenance": provenance_served(ctx, &loaded)
    }))
}

// ---------------------------------------------------------------------
// BR wave 2, B: holdings beyond the SR (JOLux: treaties, consultations,
// AS/BBl, drafts)
// ---------------------------------------------------------------------

/// `fedlex.find_treaties` — treaty processes (`jolux:TreatyProcess`)
/// by a word of their title and/or a partner country (vocabulary
/// IRI) and/or bilaterality; newest signature first; one row beyond
/// the cap. Query in the vendored JLX-TRT-02 shape, extended by the
/// title filter. Candidates: a HINT.
pub fn find_treaties(
    ctx: &Ctx,
    query: Option<&str>,
    country: Option<&str>,
    bilateral: Option<bool>,
    limit: Option<u32>,
) -> Result<Value> {
    let limit = limit.unwrap_or(20).clamp(1, MAX_TREATY_HITS);
    let q = query.map(str::trim).filter(|q| !q.is_empty());
    if q.is_none() && country.is_none() && bilateral.is_none() {
        return Ok(invalid("give at least one of query, country, bilateral"));
    }
    let mut filters = String::new();
    if let Some(c) = country {
        if !c.starts_with(VOCABULARY_BASE)
            || c.chars()
                .any(|ch| ch.is_whitespace() || matches!(ch, '<' | '>' | '"'))
        {
            return Ok(invalid(
                "country must be a Fedlex country vocabulary IRI (fedlex.resolve_vocabulary_label «country», query «Deutschland»)",
            ));
        }
        filters.push_str(&format!("  ?process jolux:treatyPartyCountry <{c}> .\n"));
    }
    if let Some(b) = bilateral {
        filters.push_str(&format!("  ?process jolux:bilateral {b} .\n"));
    }
    let title_filter = match q {
        Some(q) => format!(
            " && CONTAINS(LCASE(STR(?title)), \"{}\")",
            sparql_escape(&q.to_lowercase())?
        ),
        None => String::new(),
    };
    let sparql = format!(
        "{PREFIXES}SELECT DISTINCT ?process ?title (LANG(?title) AS ?lang) ?sigDate WHERE {{\n\
         ?process a jolux:TreatyProcess ;\n\
                  jolux:titleTreaty ?title .\n\
         {filters}\
         OPTIONAL {{ ?process jolux:treatySignatureDate ?sigDate }}\n\
         FILTER(STR(?title) != \"\"{title_filter})\n\
         }} ORDER BY DESC(?sigDate) LIMIT {}",
        (limit + 1) * 5
    );
    let key = format!(
        "find_treaties:{}:{}:{}:{limit}",
        q.unwrap_or("-"),
        country.unwrap_or("-"),
        bilateral
            .map(|b| b.to_string())
            .unwrap_or_else(|| "-".into())
    );
    let value = match ctx.backend.select(&key, &sparql) {
        Ok(v) => v,
        Err(e) => return Ok(backend_refusal(&e)),
    };
    let rows = bindings_or_refuse!(value);
    let mut processes: Vec<(String, serde_json::Map<String, Value>, Option<String>)> = Vec::new();
    for b in rows {
        let Some(p) = binding_str(b, "process") else {
            continue;
        };
        let entry = match processes.iter_mut().find(|(uri, ..)| uri == p) {
            Some(e) => e,
            None => {
                processes.push((
                    p.to_string(),
                    serde_json::Map::new(),
                    binding_str(b, "sigDate").map(str::to_string),
                ));
                processes.last_mut().expect("just pushed")
            }
        };
        if let Some(t) = binding_str(b, "title") {
            let lang = binding_str(b, "lang").unwrap_or("und").to_string();
            entry.1.entry(lang).or_insert_with(|| json!(t));
        }
    }
    let truncated = processes.len() as u32 > limit;
    processes.truncate(limit as usize);
    let hits: Vec<Value> = processes
        .iter()
        .map(|(uri, titles, date)| {
            json!({
                "process": uri,
                "title": preferred(titles).map(|(_, v)| v.clone()),
                "titles": titles,
                "signature_date": date,
            })
        })
        .collect();
    Ok(json!({
        "query": q, "country": country, "bilateral": bilateral,
        "hits": hits,
        "returned": hits.len(),
        "limit": limit,
        "truncated": truncated,
        "note": "treaty PROCESSES (eli/treaty/…) — the treaty text in the SR is a consolidation abstract of its own (resolve_sr with the 0.xxx number)",
        "kind": "hint",
        "provenance": provenance(ctx, &ctx.today)
    }))
}

/// `fedlex.get_treaty_info` — the profile of a treaty process: title
/// (language-preferred), signature date and place, bilateral flag,
/// partner countries, the approving federal decree (vendored
/// JLX-TRT-01 over the bridge). A norm about the process node.
pub fn get_treaty_info(ctx: &Ctx, eli: &str, lang: Option<&str>) -> Result<Value> {
    let eli = match iri_safe(eli) {
        Ok(e) => e,
        Err(e) => return Ok(invalid(&format!("{e:#}"))),
    };
    let language = match label_lang(lang) {
        Ok(l) => l,
        Err(refusal) => return Ok(refusal),
    };
    let client = KeyedClient::new(
        &ctx.backend,
        format!("get_treaty_info:{eli}:{}", language.tag()),
    );
    let info = match run(fedlex_jolux::get_treaty_info(&client, eli, language)) {
        Ok(i) => i,
        Err(e) => return Ok(jolux_refusal(&e, eli)),
    };
    Ok(json!({
        "treaty": {
            "process": info.process_uri,
            "title": info.title,
            "signature_date": info.signature_date,
            "signature_place": info.signature_place,
            "bilateral": info.bilateral,
            "party_countries": info.party_countries,
            "approbation_act": info.approbation_act,
        },
        "note": "countries are vocabulary IRIs — decode with fedlex.resolve_vocabulary_label («country»); \
                 reservations and entry into force live in the treaty's SR text, not on the process node",
        "kind": "norm",
        "provenance": provenance(ctx, &ctx.today)
    }))
}

fn drafts_of(ctx: &Ctx, eli: &str) -> Result<std::result::Result<Vec<fedlex_jolux::Draft>, Value>> {
    let rel = match relative_eli(eli) {
        Ok(r) => r,
        Err(refusal) => return Ok(Err(refusal)),
    };
    let as_of = match valid_as_of(&ctx.today) {
        Ok(d) => d,
        Err(refusal) => return Ok(Err(refusal)),
    };
    let client = KeyedClient::new(&ctx.backend, format!("get_drafts:{eli}"));
    match run(fedlex_jolux::get_drafts(&client, &rel, as_of)) {
        Ok(response) => Ok(Ok(response.into_parts().0)),
        Err(e) => Ok(Err(jolux_refusal(&e, eli))),
    }
}

/// `fedlex.get_drafts` — the legislative drafts (`eli/proj/…`) an act
/// came from, with the Curia Vista business number (vendored
/// JLX-GEN-01 over the bridge). The entry into the genesis: draft
/// IRIs feed `get_consultations`. A norm; an empty list for a known
/// act is an answer, an unknown act is not-found.
pub fn get_drafts(ctx: &Ctx, eli: &str) -> Result<Value> {
    let eli = match iri_safe(eli) {
        Ok(e) => e,
        Err(e) => return Ok(invalid(&format!("{e:#}"))),
    };
    let drafts = match drafts_of(ctx, eli)? {
        Ok(d) => d,
        Err(refusal) => return Ok(refusal),
    };
    if drafts.is_empty() {
        let profile = get_law_metadata(ctx, eli, None)?;
        if profile.get("error").is_some() {
            return Ok(profile);
        }
    }
    let list: Vec<Value> = drafts
        .iter()
        .map(|d| {
            json!({
                "draft": d.uri,
                "draft_id": d.draft_id,
                "parliament_draft_id": d.parliament_draft_id,
                "resulting_resources": d.resulting_resources,
            })
        })
        .collect();
    Ok(json!({
        "eli": eli,
        "drafts": list,
        "total": list.len(),
        "note": "parliament_draft_id is the Curia Vista business number; a draft IRI is the entry to fedlex.get_consultations",
        "kind": "norm",
        "provenance": provenance(ctx, &ctx.today)
    }))
}

/// The consultation query: the vendored JLX-GEN-02 shape, widened at
/// BS to the second path the graph really carries (live probe on the
/// nDSG, recorded). A parliamentary draft — what `get_drafts` resolves
/// through `jolux:basicAct` — reaches its consultation through a
/// legislative task: `draftHasLegislativeTask` → task →
/// `legislativeTaskHasResultingLegalResource` → the `Consultation`
/// (`eli/dl/proj/8022/0491/event/5` → `eli/dl/proj/6016/61/cons_1`,
/// the 2016/17 consultation of the DSG revision). The direct
/// `draftHasTask` edge the vendored primitive walks sits on the
/// consultation dossier's OWN draft node (`eli/dl/proj/6016/61`), which
/// the act never points at — hence BR's empty answers. Both paths in
/// one UNION; dates and institution on the sub-tasks (the vendored
/// trap J10.2), the title on the consultation. «from»-free for the
/// federal WAF.
fn consultations_query(draft: &str) -> String {
    format!(
        "{PREFIXES}SELECT ?cons ?status ?title ?start ?end ?inst WHERE {{\n\
         {{ <{draft}> jolux:draftHasTask ?cons }}\n\
         UNION\n\
         {{ <{draft}> jolux:draftHasLegislativeTask ?task .\n\
            ?task jolux:legislativeTaskHasResultingLegalResource ?cons }}\n\
         ?cons a jolux:Consultation .\n\
         OPTIONAL {{ ?cons jolux:consultationStatus ?status }}\n\
         OPTIONAL {{ ?cons jolux:eventTitle ?title FILTER(lang(?title) = \"de\") }}\n\
         OPTIONAL {{ ?cons jolux:hasSubTask ?sub .\n\
           OPTIONAL {{ ?sub jolux:eventStartDate ?start }}\n\
           OPTIONAL {{ ?sub jolux:eventEndDate ?end }}\n\
           OPTIONAL {{ ?sub jolux:institutionInChargeOfTheEvent ?inst }} }}\n\
         }} LIMIT 60"
    )
}

/// `fedlex.get_consultations` — the consultation procedures
/// (Vernehmlassungen) of an act's drafts, or of one draft: title,
/// status, dates and the institution in charge (own query in the
/// vendored JLX-GEN-02 shape, widened to the legislative-task path —
/// see [`consultations_query`]; the dates live on the consultation's
/// sub-tasks). Entry by act ELI (its drafts are resolved first, at
/// most five) or by draft IRI; `status` filters by the status IRI or
/// its last segment. Candidates in the genesis: a HINT — a
/// consultation is never law in force.
pub fn get_consultations(
    ctx: &Ctx,
    eli: Option<&str>,
    draft: Option<&str>,
    status: Option<&str>,
    limit: Option<u32>,
) -> Result<Value> {
    let limit = limit.unwrap_or(20).clamp(1, 50) as usize;
    let drafts: Vec<String> = match (draft, eli) {
        (Some(d), _) => {
            let d = d.trim();
            if !d.starts_with("https://fedlex.data.admin.ch/eli/")
                || d.chars()
                    .any(|c| c.is_whitespace() || matches!(c, '<' | '>' | '"'))
            {
                return Ok(invalid(
                    "draft must be a Fedlex draft IRI (…/eli/proj/… or …/eli/dl/proj/…)",
                ));
            }
            vec![d.to_string()]
        }
        (None, Some(e)) => {
            let e = match iri_safe(e) {
                Ok(e) => e,
                Err(err) => return Ok(invalid(&format!("{err:#}"))),
            };
            match drafts_of(ctx, e)? {
                Ok(d) => d.into_iter().map(|d| d.uri).collect(),
                Err(refusal) => return Ok(refusal),
            }
        }
        (None, None) => return Ok(invalid("give eli (the act) or draft (a draft IRI)")),
    };
    let drafts_total = drafts.len();
    let mut consultations = Vec::new();
    for d in drafts.iter().take(MAX_CONSULTATION_DRAFTS) {
        let value = match ctx
            .backend
            .select(&format!("get_consultations:{d}"), &consultations_query(d))
        {
            Ok(v) => v,
            Err(e) => return Ok(backend_refusal(&e)),
        };
        let bindings = bindings_or_refuse!(value);
        // One row per consultation × sub-task: fold the sub-task facts
        // onto the consultation, first value wins (the opening phase
        // carries the dates, the result phase only the institution).
        let mut found: Vec<Value> = Vec::new();
        for b in bindings {
            let Some(cons) = binding_str(b, "cons") else {
                continue;
            };
            let entry = match found.iter().position(|c| c["consultation"] == cons) {
                Some(i) => &mut found[i],
                None => {
                    found.push(json!({
                        "consultation": cons,
                        "draft": d,
                        "title": Value::Null,
                        "status": Value::Null,
                        "start_date": Value::Null,
                        "end_date": Value::Null,
                        "institution": Value::Null,
                    }));
                    found.last_mut().expect("just pushed")
                }
            };
            for (field, var) in [
                ("title", "title"),
                ("status", "status"),
                ("start_date", "start"),
                ("end_date", "end"),
                ("institution", "inst"),
            ] {
                if entry[field].is_null() {
                    if let Some(v) = binding_str(b, var) {
                        entry[field] = json!(v);
                    }
                }
            }
        }
        for c in found {
            if let Some(s) = status {
                let matches = c["status"]
                    .as_str()
                    .is_some_and(|st| st == s || st.rsplit('/').next() == Some(s));
                if !matches {
                    continue;
                }
            }
            consultations.push(c);
        }
    }
    let total = consultations.len();
    consultations.truncate(limit);
    Ok(json!({
        "eli": eli,
        "drafts_considered": drafts.iter().take(MAX_CONSULTATION_DRAFTS).collect::<Vec<_>>(),
        "drafts_total": drafts_total,
        "drafts_truncated": drafts_total > MAX_CONSULTATION_DRAFTS,
        "consultations": consultations,
        "total": total,
        "returned": consultations.len(),
        "truncated": consultations.len() < total,
        "note": "status is a vocabulary IRI (consultation-status) — decode with fedlex.resolve_vocabulary_label; \
                 title in German; the documents of a consultation: fedlex.get_consultation_documents; \
                 an empty list for a known draft means the graph links no consultation to it on either path \
                 (draftHasTask, or draftHasLegislativeTask → legislativeTaskHasResultingLegalResource)",
        "kind": "hint",
        "provenance": provenance(ctx, &ctx.today)
    }))
}

/// The sub-task documents of a consultation, cap+1 on the rows;
/// «from»-free for the federal WAF like [`consultations_query`].
fn consultation_documents_query(consultation: &str) -> String {
    format!(
        "{PREFIXES}SELECT ?doc ?role ?kind ?title WHERE {{\n\
         <{consultation}> jolux:hasSubTask ?task .\n\
         ?task ?role ?doc .\n\
         FILTER(?role IN (jolux:opinionIsAboutDraftDocument, jolux:opinionHasDraftRelatedDocument))\n\
         ?doc a ?kind .\n\
         FILTER(?kind != jolux:Work)\n\
         OPTIONAL {{ ?doc jolux:title ?title FILTER(lang(?title) = \"de\") }}\n\
         }} LIMIT {}",
        MAX_CONSULTATION_DOCUMENTS + 1
    )
}

/// `fedlex.get_consultation_documents` — the documents of one
/// consultation: what was put into consultation (`role: draft` — the
/// Vorlage), what accompanies it (`role: related` — the explanatory
/// report, cover letters, the address list, the result report), and
/// the position statements and result publications linked by
/// `isOpinionOf` (`role: opinion`, the vendored JLX-GEN-03 shape over
/// the bridge). The first two hang under the consultation's sub-tasks
/// (`hasSubTask` → `opinionIsAboutDraftDocument` |
/// `opinionHasDraftRelatedDocument`) — the shape the live probe at BS
/// found on the DSG revision, where the vendored path alone answered
/// nothing. Two queries, one key each: the vendored shape keeps the
/// tool's own key (its meaning since BR), the sub-task query runs
/// under `<key>:tasks`. A norm about that consultation's record.
pub fn get_consultation_documents(ctx: &Ctx, consultation: &str) -> Result<Value> {
    let c = consultation.trim();
    if !c.starts_with("https://fedlex.data.admin.ch/eli/")
        || c.chars()
            .any(|ch| ch.is_whitespace() || matches!(ch, '<' | '>' | '"'))
    {
        return Ok(invalid(
            "consultation must be a Fedlex consultation IRI (from fedlex.get_consultations)",
        ));
    }
    let key = format!("get_consultation_documents:{c}");
    let tasks_key = format!("{key}:tasks");
    let query = consultation_documents_query(c);
    let value = match ctx.backend.select(&tasks_key, &query) {
        Ok(v) => v,
        Err(e) => return Ok(backend_refusal(&e)),
    };
    let bindings = bindings_or_refuse!(value);
    // cap+1 is measured on the ROWS the endpoint returned — a document
    // may occupy several rows (two types, two titles) and the LIMIT
    // cuts rows, so a distinct-document count would under-report.
    let truncated = bindings.len() > MAX_CONSULTATION_DOCUMENTS;
    let mut list: Vec<Value> = Vec::new();
    for b in bindings {
        let (Some(doc), Some(role), Some(kind)) = (
            binding_str(b, "doc"),
            binding_str(b, "role"),
            binding_str(b, "kind"),
        ) else {
            continue;
        };
        if list.iter().any(|d| d["document"] == doc) {
            continue;
        }
        let role = if role.ends_with("opinionIsAboutDraftDocument") {
            "draft"
        } else {
            "related"
        };
        list.push(json!({
            "document": doc,
            "role": role,
            "kind": kind.rsplit('#').next().unwrap_or(kind),
            "title": binding_str(b, "title"),
        }));
    }
    list.truncate(MAX_CONSULTATION_DOCUMENTS);
    let under_tasks = list.len();
    // The vendored shape: position statements and result publications
    // that name the consultation (or its tasks) as what they are an
    // opinion of — under the tool's own key, as since BR.
    let client = KeyedClient::new(&ctx.backend, key);
    let opinions = match run(fedlex_jolux::get_consultation_documents(&client, c)) {
        Ok(d) => d,
        Err(e) => return Ok(jolux_refusal(&e, c)),
    };
    let opinions_total = opinions.len();
    for d in &opinions {
        if !list.iter().any(|x| x["document"] == d.uri.as_str()) {
            list.push(json!({
                "document": d.uri,
                "role": "opinion",
                "kind": d.kind,
                "title": Value::Null,
            }));
        }
    }
    let note = if list.is_empty() {
        "no document under the consultation's sub-tasks and none linked by isOpinionOf — either the \
         graph carries no documents for it or the IRI is not a consultation (fedlex.get_consultations \
         hands out the IRIs)"
    } else {
        "role: draft = the text put into consultation, related = report, cover letter, address list, \
         result report, opinion = position statements/result publications (vendored isOpinionOf shape); \
         titles in German where the graph has them; the files themselves are not fetched"
    };
    Ok(json!({
        "consultation": c,
        "documents": list,
        "total": list.len(),
        "under_tasks": under_tasks,
        "opinions": opinions_total,
        "truncated": truncated || opinions_total >= 200,
        "truncation_basis": "sub-task documents capped at 200 (cap+1 requested on the rows, so measured); \
                             opinions at the vendored primitive's LIMIT 200 — not a count",
        "note": note,
        "kind": "norm",
        "provenance": provenance(ctx, &ctx.today)
    }))
}

/// `fedlex.get_oc_act` — the legally binding basic act in the Official
/// Compilation (AS/RO) behind a consolidation abstract: its ELI,
/// publication date, genre, responsible office, memorial (vendored
/// JLX-PUB-01 over the bridge). A norm.
pub fn get_oc_act(ctx: &Ctx, eli: &str) -> Result<Value> {
    let eli = match iri_safe(eli) {
        Ok(e) => e,
        Err(e) => return Ok(invalid(&format!("{e:#}"))),
    };
    if eli.contains("/eli/oc/") {
        return Ok(invalid(
            "get_oc_act takes the act's consolidation ELI (…/eli/cc/…) and answers its AS publication — \
             you gave the AS publication itself",
        ));
    }
    let rel = match relative_eli(eli) {
        Ok(r) => r,
        Err(refusal) => return Ok(refusal),
    };
    let as_of = match valid_as_of(&ctx.today) {
        Ok(d) => d,
        Err(refusal) => return Ok(refusal),
    };
    let client = KeyedClient::new(&ctx.backend, format!("get_oc_act:{eli}"));
    let act = match run(fedlex_jolux::get_oc_act(&client, &rel, as_of)) {
        Ok(response) => response.into_parts().0,
        Err(e) => return Ok(jolux_refusal(&e, eli)),
    };
    Ok(json!({
        "eli": eli,
        "oc": act.oc_uri,
        "publication_date": act.publication_date,
        "genre": act.genre,
        "genre_label": act.genre_label,
        "responsible_office": act.responsible_office,
        "memorial": act.memorial,
        "note": "the AS publication is the legally binding text; the consolidation is a working aid. \
                 The AS issue and its other acts: fedlex.get_memorial(oc)",
        "kind": "norm",
        "provenance": provenance(ctx, &ctx.today)
    }))
}

/// `fedlex.get_memorial` — the AS/BBl issue (memorial) an Official
/// Compilation act was published in, and the acts in that issue
/// (vendored JLX-PUB-02 over the bridge; one row beyond the cap). A
/// norm about the issue.
pub fn get_memorial(ctx: &Ctx, eli: &str, limit: Option<u32>) -> Result<Value> {
    let eli = match iri_safe(eli) {
        Ok(e) => e,
        Err(e) => return Ok(invalid(&format!("{e:#}"))),
    };
    if eli.contains("/eli/cc/") {
        return Ok(invalid(
            "get_memorial takes the AS publication ELI (…/eli/oc/…) — get it from fedlex.get_oc_act(act), field oc",
        ));
    }
    let limit = limit.unwrap_or(20).clamp(1, MAX_MEMORIAL_ACTS);
    let rel = match relative_eli(eli) {
        Ok(r) => r,
        Err(refusal) => return Ok(refusal),
    };
    let client = KeyedClient::new(&ctx.backend, format!("get_memorial:{eli}:{limit}"));
    let memorial = match run(fedlex_jolux::get_memorial(&client, &rel, limit + 1)) {
        Ok(m) => m,
        Err(e) => return Ok(jolux_refusal(&e, eli)),
    };
    let truncated = memorial.acts.len() as u32 > limit;
    let acts: Vec<&String> = memorial.acts.iter().take(limit as usize).collect();
    Ok(json!({
        "oc": eli,
        "memorial": memorial.uri,
        "acts": acts,
        "returned": acts.len(),
        "limit": limit,
        "truncated": truncated,
        "kind": "norm",
        "provenance": provenance(ctx, &ctx.today)
    }))
}

/// `fedlex.get_fga_documents` — the Federal Gazette (BBl/FF) documents
/// of an act's genesis: dispatches (Botschaften) and reports, with
/// genre and publication date (vendored JLX-PUB-03 over the bridge).
/// A norm; an empty list for a known act is an answer.
pub fn get_fga_documents(ctx: &Ctx, eli: &str) -> Result<Value> {
    let eli = match iri_safe(eli) {
        Ok(e) => e,
        Err(e) => return Ok(invalid(&format!("{e:#}"))),
    };
    let rel = match relative_eli(eli) {
        Ok(r) => r,
        Err(refusal) => return Ok(refusal),
    };
    let as_of = match valid_as_of(&ctx.today) {
        Ok(d) => d,
        Err(refusal) => return Ok(refusal),
    };
    let client = KeyedClient::new(&ctx.backend, format!("get_fga_documents:{eli}"));
    let docs = match run(fedlex_jolux::get_fga_documents(&client, &rel, as_of)) {
        Ok(response) => response.into_parts().0,
        Err(e) => return Ok(jolux_refusal(&e, eli)),
    };
    if docs.is_empty() {
        let profile = get_law_metadata(ctx, eli, None)?;
        if profile.get("error").is_some() {
            return Ok(profile);
        }
    }
    let list: Vec<Value> = docs
        .iter()
        .map(|d| {
            json!({
                "document": d.uri,
                "genre": d.genre,
                "genre_label": d.genre_label,
                "publication_date": d.publication_date,
            })
        })
        .collect();
    Ok(json!({
        "eli": eli,
        "documents": list,
        "total": list.len(),
        "truncated": list.len() >= 50,
        "truncation_basis": "cap reached (the vendored primitive's LIMIT 50) — not a count",
        "note": "materials for interpretation (Botschaft, reports) — not law in force",
        "kind": "norm",
        "provenance": provenance(ctx, &ctx.today)
    }))
}

// ---------------------------------------------------------------------
// BT: the citation pair — check_quote and cite. No upstream
// counterpart: the platform goes beyond the reference reader here,
// because the chat's citation chain (E01/E16: no private capability)
// needs a check of wording where the norm text lies, and a canonical
// label for every place it read.
// ---------------------------------------------------------------------

/// A quote longer than this is not a quote.
const MAX_QUOTE_CHARS: usize = 20_000;

/// The normalisation both sides of a quote check pass through — the
/// quote and the norm text alike, so a difference in typography is
/// never a difference in wording: every run of whitespace (no-break
/// spaces included) becomes one space and the ends are trimmed; the
/// typographic quotation marks « » „ “ ” ‟ become the straight double
/// quote and ‚ ‘ ’ ‹ › the straight single one; soft hyphens vanish;
/// the dashes – — ‒ − and the no-break hyphen become the hyphen-minus.
/// Case is kept: a quote is verbatim or it is not.
fn normalise_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut pending_space = false;
    for ch in s.chars() {
        let mapped = match ch {
            '\u{00AD}' => continue,
            '«' | '»' | '„' | '“' | '”' | '‟' => '"',
            '‚' | '‘' | '’' | '‹' | '›' => '\'',
            '\u{2013}' | '\u{2014}' | '\u{2012}' | '\u{2212}' | '\u{2011}' => '-',
            c if c.is_whitespace() => {
                pending_space = true;
                continue;
            }
            c => c,
        };
        if pending_space {
            if !out.is_empty() {
                out.push(' ');
            }
            pending_space = false;
        }
        out.push(mapped);
    }
    out
}

/// The segments of a quote: «…», «...», «[…]» and «[...]» mark an
/// omission and split the quote; every segment must occur, in order.
fn quote_segments(quote: &str) -> Vec<String> {
    // A bracketed ellipsis WITH inner spaces is wording, not an omission
    // mark. It is parked under a sentinel through the split and restored
    // exactly as it was sent — «[ … ]» comes back as «[ … ]», never
    // rewritten to «[ ... ]» (BT′ audit).
    const PARK: char = '\u{1}';
    let mut parked: Vec<&str> = Vec::new();
    let mut kept = String::with_capacity(quote.len());
    let mut rest = quote;
    while let Some(open) = rest.find("[ ") {
        let after = &rest[open..];
        let Some(close) = after.find(']').map(|c| c + 1) else {
            break;
        };
        let span = &after[..close];
        let inner = span[2..span.len() - 1].trim();
        if inner == "..." || inner == "…" {
            kept.push_str(&rest[..open]);
            kept.push(PARK);
            parked.push(span);
            rest = &after[close..];
        } else {
            kept.push_str(&rest[..open + 2]);
            rest = &after[2..];
        }
    }
    kept.push_str(rest);
    let mut parked = parked.into_iter();
    kept.replace("[...]", "\u{0}")
        .replace("[…]", "\u{0}")
        .replace("...", "\u{0}")
        .replace('…', "\u{0}")
        .split('\u{0}')
        .map(|segment| {
            let restored: String = segment
                .split(PARK)
                .enumerate()
                .map(|(i, piece)| {
                    if i == 0 {
                        piece.to_string()
                    } else {
                        format!("{}{piece}", parked.next().unwrap_or_default())
                    }
                })
                .collect();
            normalise_quote(&restored)
        })
        .filter(|segment| !segment.is_empty())
        .collect()
}

/// How an eId resolved, for the answer to say (BV, rules X15.3 and
/// X9.4/J18.2): how many OTHER elements carry the same eId — the
/// corpus has 842 files with duplicates, so the first hit in document
/// order is a choice, not a certainty — and whether the hit was found
/// only after normalisation (`art_25a` → `art_25_a`, the JOLux
/// spelling against the manifestation's). Both are facts about the
/// lookup, and an answer that hides them claims a precision it does
/// not have.
fn eid_resolution(doc: &AknDocument, eid: &str) -> (usize, bool) {
    fedlex_akn::resolve_eid(doc, eid)
        .map(|hit| (hit.duplicates, hit.via_normalization))
        .unwrap_or((0, false))
}

/// The elements that sit DIRECTLY under `<body>` and are not part of
/// the hierarchy the vendored renderer walks: `<p>` (2'493 in the
/// corpus), `<signature>` (1'289, always at the end), `<table>` (240),
/// `<blockList>` (156) — X17.8/X18.7. They carry text, they have no
/// eId, and before BV every one of them was lost from `read_document`,
/// `get_structure` and `search_text` alike. Returned in document order
/// with the text and whether the element stands after the last
/// hierarchy child (the signature case) or before/among them.
#[derive(Debug)]
struct BodyLevelElement {
    tag: String,
    text: String,
    after_hierarchy: bool,
    /// The first line the renderer writes for the NEXT hierarchy
    /// sibling — the place this element's text is inserted before, so
    /// it keeps its position in document order (X18.7).
    anchor: Option<String>,
}

fn body_level_elements(doc: &AknDocument) -> Vec<BodyLevelElement> {
    let Some(body) = doc
        .find_all(doc.root(), "body")
        .into_iter()
        .chain(doc.find_all(doc.root(), "mainBody"))
        .next()
    else {
        return Vec::new();
    };
    let children: Vec<_> = doc.children(body).collect();
    let last_hierarchy = children
        .iter()
        .rposition(|node| fedlex_akn::is_hierarchy_tag(doc.tag(*node)));
    children
        .iter()
        .enumerate()
        .filter(|(_, node)| !fedlex_akn::is_hierarchy_tag(doc.tag(**node)))
        .filter_map(|(index, node)| {
            let text = doc.text_of(*node).trim().to_string();
            let after_hierarchy = last_hierarchy.is_none_or(|last| index > last);
            let anchor = (!after_hierarchy)
                .then(|| {
                    children
                        .iter()
                        .skip(index + 1)
                        .find(|next| fedlex_akn::is_hierarchy_tag(doc.tag(**next)))
                        .and_then(|next| render_anchor(doc, *next))
                })
                .flatten();
            (!text.is_empty()).then(|| BodyLevelElement {
                tag: doc.tag(*node).to_string(),
                text,
                after_hierarchy,
                anchor,
            })
        })
        .collect()
}

/// The first line the vendored renderer writes for a hierarchy element:
/// its «num heading» label where it has hierarchy children below it,
/// else the first line of its text (a leaf is rendered en bloc). That
/// line is the anchor body-level text is inserted BEFORE (X18.7).
fn render_anchor(doc: &AknDocument, node: fedlex_akn::NodeId) -> Option<String> {
    let has_hierarchy_children = doc
        .children(node)
        .any(|child| fedlex_akn::is_hierarchy_tag(doc.tag(child)));
    if has_hierarchy_children {
        let num = doc.find_child(node, "num").map(|n| doc.text_of(n));
        let heading = doc.find_child(node, "heading").map(|h| doc.text_of(h));
        let label = [num, heading]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" ");
        (!label.trim().is_empty()).then_some(label)
    } else {
        doc.text_of(node)
            .lines()
            .find(|line| !line.trim().is_empty())
            .map(str::to_string)
    }
}

/// Renders the body-level elements INTO the document markdown (X18.7):
/// what stands after the last hierarchy child goes to the end in
/// document order, what stands AMONG the hierarchy is inserted before
/// the line its next hierarchy sibling opens with — its own place, not
/// a prologue. Pure, so the order is provable without a backend.
fn render_body_level(markdown: String, extras: &[BodyLevelElement]) -> String {
    let mut out = markdown;
    // Back to front: an insertion never moves an anchor still to find.
    for extra in extras.iter().filter(|e| !e.after_hierarchy).rev() {
        match extra.anchor.as_deref().and_then(|anchor| out.find(anchor)) {
            Some(pos) => {
                let line_start = out[..pos].rfind('\n').map_or(0, |i| i + 1);
                out.insert_str(line_start, &format!("{}\n\n", extra.text));
            }
            // No anchor in the rendered text: the element still belongs
            // to the answer — at the top, where a prologue stands.
            None => out = format!("{}\n\n{out}", extra.text),
        }
    }
    for extra in extras.iter().filter(|e| e.after_hierarchy) {
        out.push_str("\n\n");
        out.push_str(&extra.text);
    }
    out
}

/// The summary an answer carries about them: one row per element with
/// its tag, its length and where it stands.
fn body_level_json(elements: &[BodyLevelElement]) -> Value {
    Value::Array(
        elements
            .iter()
            .map(|e| {
                json!({
                    "tag": e.tag,
                    "chars": e.text.chars().count(),
                    "position": if e.after_hierarchy { "after the hierarchy" } else { "among the hierarchy" },
                    "rendered": if e.after_hierarchy {
                        "at the end, in document order"
                    } else if e.anchor.is_some() {
                        "at its position among the hierarchy"
                    } else {
                        "at the beginning — the renderer writes no line to place it before"
                    },
                })
            })
            .collect(),
    )
}

/// The not-found refusal of an eId the manifestation does not carry —
/// the shape `read_article` answers, shared by the pair.
fn element_not_found(eli_version: &str, eid: &str, error: &fedlex_akn::AknError) -> Value {
    json!({
        "error": "not-found",
        "subject": format!("{eli_version}#{eid}"),
        "detail": format!("{error} — fedlex.get_structure lists the eIds this version has, \
                           fedlex.search_text finds the place by a word")
    })
}

/// Resolves an eId to its element — and an annex WRAPPER (`annex_3`,
/// `annex_u1`, what `parse_reference`'s hint names) to the annex's
/// first level (`annex_3/lvl_u1`), because the wrapper is not an
/// element of the manifestation the vendored resolver addresses while
/// the first level carries the whole annex body (BT′, decided: the
/// hint stays a prefix, the tools resolve it and answer with the eId
/// they actually read).
fn element_or_first_level(
    doc: &AknDocument,
    eid: &str,
    as_of: ValidAsOf,
) -> std::result::Result<fedlex_akn::ElementText, fedlex_akn::AknError> {
    match fedlex_akn::get_element_text(doc, eid, as_of) {
        Ok(response) => Ok(response.into_parts().0),
        Err(e) if eid.starts_with("annex_") && !eid.contains('/') => {
            let Some(first) = first_level_of(doc, eid) else {
                return Err(e);
            };
            fedlex_akn::get_element_text(doc, &first, as_of)
                .map(|response| response.into_parts().0)
                .map_err(|_| e)
        }
        Err(e) => Err(e),
    }
}

/// The first level under an annex wrapper, READ from the manifestation
/// in document order — never assumed to be `lvl_u1` (BT′ audit: every
/// recorded annex happens to write that, and an assumption that holds
/// four times is still an assumption).
fn first_level_of(doc: &AknDocument, annex: &str) -> Option<String> {
    let prefix = format!("{annex}/");
    doc.all_eids()
        .filter(|(eid, _)| eid.starts_with(&prefix) && !eid[prefix.len()..].contains('/'))
        .filter_map(|(eid, nodes)| nodes.first().map(|node| (*node, eid.to_string())))
        .min_by_key(|(node, _)| *node)
        .map(|(_, eid)| eid)
}

/// `fedlex.check_quote` — does a quote stand in the norm text of one
/// element? The judge-free core metric of the citation chain, as a
/// platform capability: the wording a model (or a person) claims to
/// have read is checked where the text lies, against the very
/// manifestation `read_article` served — through the cache, so a check
/// after a read costs no token. Whitespace, quotation marks and dashes
/// are normalised on both sides; an omission mark splits the quote into
/// segments that must occur in order. The answer is about WORDING and
/// nothing else: never whether the statement is true, complete or
/// still in force. A quote that is not there is `verified: false` with
/// its missing segments — an answer, not an error.
pub fn check_quote(
    ctx: &Ctx,
    eli_version: &str,
    eid: &str,
    quote: &str,
    lang: Option<&str>,
) -> Result<Value> {
    if quote.trim().is_empty() {
        return Ok(invalid(
            "quote must be the wording to check — what a model or a person claims to have read",
        ));
    }
    if quote.chars().count() > MAX_QUOTE_CHARS {
        return Ok(invalid(
            "quote must be a quote, not a document (at most 20 000 characters)",
        ));
    }
    let segments = quote_segments(quote);
    if segments.is_empty() {
        return Ok(invalid(
            "quote carries no words — only omission marks or whitespace",
        ));
    }
    let eid = match valid_eid(eid) {
        Ok(e) => e,
        Err(refusal) => return Ok(refusal),
    };
    let loaded = loaded_or_refuse!(load_version(ctx, eli_version, lang));
    let element = match element_or_first_level(&loaded.doc, eid, loaded.as_of) {
        Ok(element) => element,
        Err(e) => return Ok(element_not_found(eli_version, eid, &e)),
    };
    let (duplicates, via_normalisation) = eid_resolution(&loaded.doc, &element.eid);
    let text = normalise_quote(&element.text);
    let mut cursor = 0usize;
    let mut all_found = true;
    let mut report = Vec::with_capacity(segments.len());
    for segment in &segments {
        match text[cursor..].find(segment.as_str()) {
            Some(offset) => {
                let at_byte = cursor + offset;
                let at = text[..at_byte].chars().count();
                cursor = at_byte + segment.len();
                report.push(json!({"text": segment, "found": true, "at": at}));
            }
            None => {
                all_found = false;
                report.push(json!({"text": segment, "found": false}));
            }
        }
    }
    Ok(json!({
        "verified": all_found,
        "segments": report,
        "segments_total": segments.len(),
        "eid": element.eid,
        "eid_duplicates": duplicates,
        "eid_via_normalisation": via_normalisation,
        "element_kind": element.kind,
        "eli_version": eli_version,
        "lang": loaded.lang,
        "text_length": text.chars().count(),
        "note": "verified says only that the wording occurs, in order, in the text of the \
                 element exactly as read_article serves it — the article's number and heading, \
                 the paragraph numbers and the list letters included, footnotes excluded \
                 (whitespace, quotation marks and dashes normalised, case kept) — never that a \
                 statement is true, complete or in force; an annex is served from its first \
                 level, so its text begins with that level's own number and heading and carries \
                 no «Anhang n»; «…», «...», «[…]», «[...]» in the quote mark an omission and \
                 split it into segments («[ ... ]» and «[ … ]» with inner spaces do not — they \
                 stay wording, verbatim as sent); an ellipsis that stands in the norm text itself \
                 (a repealed paragraph reads «…») is not quotable wording; eid_duplicates > 0 \
                 means another element carries the same address and the wording may sit in that \
                 one instead (X15.3: 842 corpus files carry duplicates)",
        "kind": "norm",
        "provenance": provenance_served(ctx, &loaded)
    }))
}

/// The components an eId path names — the inverse of
/// [`eid_candidate`], on the same spelling: `art_25_a` → article
/// «25a», `para_1_bis` → paragraph «1bis», `lbl_b` → letter, a numeric
/// `lbl_2` below it → number, `annex_3` → annex; a bare `para` (an
/// article's single, unnumbered paragraph) adds nothing; `lvl_*` and
/// `listintro` are generic and add nothing. `None` for an eId that
/// names no citable place (a section, a chapter, the preamble).
#[derive(Default, Debug)]
struct EidComponents {
    article: Option<String>,
    paragraph: Option<String>,
    letter: Option<String>,
    number: Option<String>,
    annex: Option<String>,
    unnumbered_annex: bool,
}

/// `25_a` → `25a`, `1_bis` → `1bis`, `7` → `7`.
fn join_number(rest: &str) -> String {
    rest.replace('_', "")
}

/// Why an eId has no article-style label — each shape gets its true
/// reason (BT′): a structural element is not a citable place, a
/// transitional provision is cited by its heading, anything else has
/// no citation grammar yet.
#[derive(Debug, PartialEq, Eq)]
enum EidShape {
    Structural,
    Transitional,
    /// The segment that has no citation grammar — the offending one,
    /// not the first (BT′ audit).
    Unknown(String),
}

/// The structural elements a citation never addresses — an outline
/// level, not a place. The list is closed and the refusal names it.
const STRUCTURAL_EIDS: [&str; 4] = ["preamble", "body", "main", "conclusions"];
const STRUCTURAL_PREFIXES: [&str; 7] =
    ["sec_", "chp_", "chap_", "part_", "book_", "tit_", "title_"];

/// Reads ONE eId segment into the components it names. `Ok(true)` means
/// the segment names a citable place of its own (an article, an annex);
/// `Err` carries the segment itself when it names no citable part.
fn read_eid_segment(segment: &str, c: &mut EidComponents) -> std::result::Result<bool, String> {
    if let Some(rest) = segment.strip_prefix("art_") {
        c.article = Some(join_number(rest));
        return Ok(true);
    }
    if segment == "para" {
        // one unnumbered paragraph: the article is the address
        return Ok(false);
    }
    if let Some(rest) = segment.strip_prefix("para_") {
        c.paragraph = Some(join_number(rest));
        return Ok(false);
    }
    if let Some(rest) = segment.strip_prefix("lbl_") {
        let joined = join_number(rest);
        if rest
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_alphabetic())
            && is_letter_token(&joined)
        {
            if c.letter.is_none() {
                c.letter = Some(joined);
            }
            return Ok(false);
        }
        if rest.chars().next().is_some_and(|ch| ch.is_ascii_digit()) {
            c.number = Some(joined);
            return Ok(false);
        }
        return Err(segment.to_string());
    }
    if let Some(rest) = segment.strip_prefix("annex_") {
        if rest.starts_with('u') {
            c.unnumbered_annex = true;
        } else {
            c.annex = Some(join_number(rest));
        }
        return Ok(true);
    }
    if segment.starts_with("lvl_") || segment.starts_with("list") {
        // generic level / list-intro eIds: no citation part
        return Ok(false);
    }
    Err(segment.to_string())
}

/// The parts of a citation below the article or annex, in the order a
/// Swiss citation writes them. Shared by the article branch and the
/// transitional branch, so both write «Abs. 2 Bst. a» alike (BT′).
fn sub_address_parts(lang: &str, c: &EidComponents) -> Vec<String> {
    let mut parts = Vec::new();
    if let Some(p) = &c.paragraph {
        parts.push(format!(
            "{} {p}",
            citation_word(lang, CitationPart::Paragraph)
        ));
    }
    if let Some(l) = &c.letter {
        parts.push(format!("{} {l}", citation_word(lang, CitationPart::Letter)));
    }
    if let Some(n) = &c.number {
        parts.push(format!("{} {n}", citation_word(lang, CitationPart::Number)));
    }
    parts
}

fn eid_components(eid: &str) -> std::result::Result<EidComponents, EidShape> {
    let mut c = EidComponents::default();
    let mut citable = false;
    let first = eid.split('/').next().unwrap_or_default();
    if first.starts_with("disp_") {
        return Err(EidShape::Transitional);
    }
    if STRUCTURAL_PREFIXES.iter().any(|p| first.starts_with(p)) || STRUCTURAL_EIDS.contains(&first)
    {
        return Err(EidShape::Structural);
    }
    for segment in eid.split('/') {
        match read_eid_segment(segment, &mut c) {
            Ok(names_a_place) => citable |= names_a_place,
            Err(offending) => return Err(EidShape::Unknown(offending)),
        }
    }
    if citable {
        Ok(c)
    } else {
        Err(EidShape::Unknown(eid.to_string()))
    }
}

/// The canonical label in a language: the parts in the order a Swiss
/// citation writes them, then the act's abbreviation — or, where the
/// graph carries no abbreviation in that language, its SR number.
fn citation_label(lang: &str, c: &EidComponents, short: Option<&str>, sr: Option<&str>) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(a) = &c.annex {
        parts.push(format!("{} {a}", citation_word(lang, CitationPart::Annex)));
    } else if c.unnumbered_annex {
        parts.push(citation_word(lang, CitationPart::Annex).to_string());
    }
    if let Some(a) = &c.article {
        parts.push(format!(
            "{} {a}",
            citation_word(lang, CitationPart::Article)
        ));
    }
    parts.extend(sub_address_parts(lang, c));
    if let Some(short) = short {
        parts.push(short.to_string());
    } else if let Some(sr) = sr {
        parts.push(format!("(SR {sr})"));
    }
    parts.join(" ")
}

/// The act's abbreviations and titles per language and its SR number
/// — one query on the abstract (`jolux:titleShort` and `jolux:title`
/// of every language expression, the taxonomy notation as
/// `resolve_sr` reads it). Fixture key `cite:<abstract-eli>`.
struct ShortProfile {
    short: std::collections::BTreeMap<String, String>,
    title: std::collections::BTreeMap<String, String>,
    sr: Option<String>,
}

fn act_short_profile(ctx: &Ctx, eli: &str) -> Result<std::result::Result<ShortProfile, Value>> {
    let query = format!(
        "{PREFIXES}SELECT ?lang ?short ?title ?sr WHERE {{\n\
         <{eli}> jolux:isRealizedBy ?e .\n\
         ?e jolux:language ?lang .\n\
         OPTIONAL {{ ?e jolux:titleShort ?short }}\n\
         OPTIONAL {{ ?e jolux:title ?title }}\n\
         OPTIONAL {{ <{eli}> jolux:classifiedByTaxonomyEntry ?t . ?t skos:notation ?sr }}\n\
         }} LIMIT 40"
    );
    let value = match ctx.backend.select(&format!("cite:{eli}"), &query) {
        Ok(v) => v,
        Err(e) => return Ok(Err(backend_refusal(&e))),
    };
    let rows = match Backend::bindings(&value) {
        Ok(b) => b,
        Err(e) => return Ok(Err(upstream(format!("{e:#}")))),
    };
    let mut profile = ShortProfile {
        short: Default::default(),
        title: Default::default(),
        sr: None,
    };
    for row in rows {
        let Some(lang) = binding_str(row, "lang").map(lang_code) else {
            continue;
        };
        if let Some(short) = binding_str(row, "short") {
            profile
                .short
                .entry(lang.clone())
                .or_insert(short.to_string());
        }
        if let Some(title) = binding_str(row, "title") {
            profile.title.entry(lang).or_insert(title.to_string());
        }
        if profile.sr.is_none() {
            profile.sr = binding_str(row, "sr").map(str::to_string);
        }
    }
    Ok(Ok(profile))
}

/// The label of a transitional provision (`disp_u<n>`), which the
/// manifestation designates by its heading rather than by a number:
/// «Schlussbestimmungen der Änderung vom 24. März 2000 KVG». A segment
/// below it is written in the SAME grammar the article branch uses —
/// `disp_u1/para_2` → «… Abs. 2 KVG», `disp_u5/para/lbl_a` → «… Bst. a
/// KVG» (BT′ audit: before this, two different places shared one
/// label). The heading sits on the provision itself, so a child
/// borrows it.
///
/// # Errors
///
/// The honest refusal text: a provision without a heading has no
/// citation grammar, and a segment that names no citable part is named
/// in the message.
fn transitional_label(
    doc: &AknDocument,
    eid: &str,
    as_of: ValidAsOf,
    lang: &str,
    act: Option<&str>,
) -> std::result::Result<String, String> {
    let head = eid.split('/').next().unwrap_or(eid);
    let heading = if head == eid {
        fedlex_akn::get_element_text(doc, eid, as_of)
            .ok()
            .and_then(|r| r.into_parts().0.heading)
    } else {
        fedlex_akn::get_element_text(doc, head, as_of)
            .ok()
            .and_then(|r| r.into_parts().0.heading)
    };
    let Some(heading) = heading
        .as_deref()
        .map(normalise_quote)
        .filter(|h| !h.is_empty())
    else {
        return Err(format!(
            "«{eid}» is a transitional provision without a heading in this manifestation — no \
             citation grammar yet; fedlex.read_article reads it"
        ));
    };
    let mut below = EidComponents::default();
    for segment in eid.split('/').skip(1) {
        if let Err(offending) = read_eid_segment(segment, &mut below) {
            return Err(format!(
                "«{eid}» has no citation grammar yet (the segment «{offending}» names no citable \
                 part below a transitional provision): fedlex.read_article reads it"
            ));
        }
    }
    let mut parts = vec![heading];
    parts.extend(sub_address_parts(lang, &below));
    if let Some(act) = act {
        parts.push(act.to_string());
    }
    Ok(parts.join(" "))
}

/// `fedlex.cite` — the canonical Fundstelle of an eId in a dated
/// consolidation: «Art. 7 Abs. 1 Bst. b LSV», «art. 7 al. 1 let. b
/// OPB», «Anhang 3 LSV». The parts come from the eId path (the
/// inverse of `parse_reference`'s grammar, the same vocabulary table),
/// the abbreviation from `jolux:titleShort` of the language's
/// expression, the SR number from the taxonomy; the element must exist
/// in the manifestation (through the cache) — a label for a place that
/// is not there would be a hint, and this is a norm. The other half of
/// the pair: `check_quote` proves the wording, `cite` names the place.
pub fn cite(ctx: &Ctx, eli_version: &str, eid: &str, lang: Option<&str>) -> Result<Value> {
    let eid = match valid_eid(eid) {
        Ok(e) => e,
        Err(refusal) => return Ok(refusal),
    };
    let components = match eid_components(eid) {
        Ok(c) => Some(c),
        Err(EidShape::Structural) => {
            return Ok(invalid(&format!(
                "cite labels articles, paragraphs, letters, numbers and annexes — «{eid}» names \
                 a structural element (section, chapter, part, book, title, preamble, body, \
                 main, conclusions); fedlex.get_structure shows the articles under it"
            )))
        }
        Err(EidShape::Unknown(offending)) => {
            return Ok(invalid(&format!(
                "«{eid}» has no citation grammar yet (the segment «{offending}» names no citable \
                 part): cite labels articles, paragraphs, letters, numbers, annexes and \
                 transitional provisions"
            )))
        }
        // A transitional provision (`disp_u<n>`) is cited by its heading.
        Err(EidShape::Transitional) => None,
    };
    let version = match parse_version(eli_version) {
        Ok(v) => v,
        Err(refusal) => return Ok(refusal),
    };
    let loaded = loaded_or_refuse!(load_version(ctx, eli_version, lang));
    let element = match element_or_first_level(&loaded.doc, eid, loaded.as_of) {
        Ok(element) => element,
        Err(e) => return Ok(element_not_found(eli_version, eid, &e)),
    };
    let (duplicates, via_normalisation) = eid_resolution(&loaded.doc, &element.eid);
    let profile = match act_short_profile(ctx, version.abstract_eli)? {
        Ok(p) => p,
        Err(refusal) => return Ok(refusal),
    };
    let short = profile.short.get(loaded.lang).map(String::as_str);
    let act = short
        .map(str::to_string)
        .or_else(|| profile.sr.as_ref().map(|sr| format!("(SR {sr})")));
    let (label, designation) = match &components {
        Some(c) => (
            citation_label(loaded.lang, c, short, profile.sr.as_deref()),
            if c.annex.is_some() || c.unnumbered_annex {
                "annex"
            } else {
                "article"
            },
        ),
        None => {
            match transitional_label(&loaded.doc, eid, loaded.as_of, loaded.lang, act.as_deref()) {
                Ok(label) => (label, "transitional-provision"),
                Err(detail) => return Ok(invalid(&detail)),
            }
        }
    };
    let c = components.unwrap_or_default();
    let annex = c
        .annex
        .as_ref()
        .map(|n| format!("{} {n}", citation_word(loaded.lang, CitationPart::Annex)));
    Ok(json!({
        "label": label,
        "designation": designation,
        "short": short,
        "sr": profile.sr,
        "article": c.article,
        "paragraph": c.paragraph,
        "letter": c.letter,
        "number": c.number,
        "annex": annex,
        "title": profile.title,
        "eli": version.abstract_eli,
        "eli_version": eli_version,
        "valid_as_of": loaded.date,
        "lang": loaded.lang,
        "eid": element.eid,
        "eid_duplicates": duplicates,
        "eid_via_normalisation": via_normalisation,
        "element_kind": element.kind,
        "heading": element.heading,
        "note": "label is the canonical Fundstelle in the manifestation language (short from \
                 jolux:titleShort; SR in brackets where the graph carries no abbreviation); \
                 annex levels carry generic eIds, so an annex is cited by its number only, an \
                 unnumbered annex (annex_u1) as «Anhang» alone with annex: null; an annex wrapper \
                 (annex_3) is resolved to its FIRST level as the manifestation orders them, and \
                 eid names what was read; a transitional provision (disp_u<n>) is cited by its \
                 heading, verbatim, with any paragraph, letter or number below it written as \
                 under an article — parse_reference has no grammar for that label; \
                 eid_duplicates > 0 means another element carries the same address, so the label \
                 may name the other one (X15.3)",
        "kind": "norm",
        "provenance": provenance_served(ctx, &loaded)
    }))
}

#[cfg(test)]
mod tests {

    /// BY point 0: the title filter is every WORD, in the same
    /// literal — and for one word the FRAGMENT is byte-for-byte the
    /// contiguous filter it replaced, which is what let the five
    /// recorded one-word windows stand (the `FILTER` around it gained
    /// two parenthesis pairs; this test pins the fragment, which is the
    /// part that decides what the endpoint is asked).
    #[test]
    fn the_title_filter_asks_for_every_word_and_for_one_word_is_unchanged() {
        let one = vec!["datenschutz".to_string()];
        assert_eq!(
            all_words_in(&one, "ft"),
            "CONTAINS(LCASE(STR(?ft)), \"datenschutz\")",
            "one word: the filter the five recorded windows were made with"
        );
        let many: Vec<String> = "bundesgesetz über die politischen rechte"
            .split_whitespace()
            .map(str::to_string)
            .collect();
        let filter = all_words_in(&many, "ft");
        assert_eq!(
            filter.matches("CONTAINS(").count(),
            5,
            "five words, five tests: {filter}"
        );
        assert_eq!(
            filter.matches(" && ").count(),
            4,
            "and they are joined by AND, so all five must stand in the SAME title: {filter}"
        );
        assert!(
            !filter.contains("bundesgesetz über"),
            "the phrase is never sent as one string — the graph writes «Bundesgesetz vom 17. \
             Dezember 1976 über …», so a contiguous match cannot find the act it names: {filter}"
        );
        // The alternative-title half is the same filter on the other
        // variable, so a popular name is matched word-wise too.
        assert_eq!(
            all_words_in(&many, "alt").matches("?alt").count(),
            5,
            "the popular-name half asks for the same words"
        );
    }
    use super::*;

    /// The normalisation makes typography irrelevant and keeps case.
    #[test]
    fn quote_normalisation_folds_typography_and_whitespace() {
        assert_eq!(
            normalise_quote("  Die  «Ausdrücke»\n„Staaten“ –\u{00A0}9\u{00AD} ‚x‘ "),
            "Die \"Ausdrücke\" \"Staaten\" - 9 'x'"
        );
        assert_eq!(normalise_quote("A—B‒C−D"), "A-B-C-D");
        assert_ne!(normalise_quote("Recht"), normalise_quote("recht"));
        assert_eq!(
            quote_segments("Jede Person … zu erhalten [...] Ende ... x"),
            vec!["Jede Person", "zu erhalten", "Ende", "x"]
        );
        assert!(quote_segments(" … [...] ").is_empty());
    }

    /// The lookup says how it resolved (BV): a duplicate eId — the
    /// corpus carries 842 such files (X15.3) — and a hit found only
    /// through normalisation (`art_25a` ↔ `art_25_a`, X9.4/J18.2).
    #[test]
    fn eid_resolution_reports_duplicates_and_normalisation() {
        // r##…##: the document itself contains `"#ch.bk"`.
        let xml = r##"<akomaNtoso xmlns="http://docs.oasis-open.org/legaldocml/ns/akn/3.0">
<act name="publicLaw"><meta><identification source="#ch.bk"><FRBRWork>
<FRBRthis value="/eli/cc/2000/1"/><FRBRuri value="/eli/cc/2000/1"/>
<FRBRdate date="2000-01-01" name="jolux:dateDocument"/><FRBRauthor href="#ch.bk"/>
<FRBRcountry value="ch"/></FRBRWork><FRBRExpression><FRBRthis value="/eli/cc/2000/1/de"/>
<FRBRuri value="/eli/cc/2000/1/de"/><FRBRlanguage language="de"/></FRBRExpression>
<FRBRManifestation><FRBRthis value="/eli/cc/2000/1/de/xml"/><FRBRformat value="xml"/>
</FRBRManifestation></identification></meta><body>
<article eId="art_1"><num>Art. 1</num><paragraph eId="art_1/para_1"><content><p>Erster.</p></content></paragraph></article>
<article eId="art_14_a"><num>Art. 14a</num><paragraph eId="art_14_a/para_1"><content><p>Eingefügt.</p></content></paragraph></article>
<article eId="art_1"><num>Art. 1</num><paragraph eId="art_1/para_9"><content><p>Doppelt vergeben.</p></content></paragraph></article>
</body></act></akomaNtoso>"##;
        let doc = AknDocument::parse(xml).expect("the synthetic document parses");
        // The duplicate: two articles carry `art_1`, the answer says so.
        assert_eq!(eid_resolution(&doc, "art_1"), (1, false));
        // Which of the two the lookup takes: the FIRST element in
        // document order, never the duplicate below it.
        let first = fedlex_akn::resolve_eid(&doc, "art_1").expect("art_1 resolves");
        let first_text = doc.text_of(first.node);
        assert!(
            first_text.contains("Erster."),
            "the first art_1 in document order, not the duplicate: {first_text}"
        );
        // The unambiguous one.
        assert_eq!(eid_resolution(&doc, "art_14_a"), (0, false));
        // The JOLux spelling finds the manifestation's, and says how.
        assert_eq!(eid_resolution(&doc, "art_14a"), (0, true));
        // An eId nobody carries resolves to nothing — read_article
        // answers not-found on its own path.
        assert_eq!(eid_resolution(&doc, "art_99"), (0, false));
    }

    /// A transitional provision is cited by its heading, and a segment
    /// below it is written in the article branch's own grammar — the
    /// two branches the recorded manifestations cannot show (a numbered
    /// paragraph, a provision without a heading) are proven here on a
    /// synthetic stub (BT′ audit).
    #[test]
    fn a_transitional_provision_is_labelled_by_its_heading_and_its_sub_address() {
        let xml = r##"<akomaNtoso xmlns="http://docs.oasis-open.org/legaldocml/ns/akn/3.0">
<act name="publicLaw"><meta><identification source="#ch.bk"><FRBRWork>
<FRBRthis value="/eli/cc/2000/2"/><FRBRuri value="/eli/cc/2000/2"/>
<FRBRdate date="2000-01-01" name="jolux:dateDocument"/><FRBRauthor href="#ch.bk"/>
<FRBRcountry value="ch"/></FRBRWork><FRBRExpression><FRBRthis value="/eli/cc/2000/2/de"/>
<FRBRuri value="/eli/cc/2000/2/de"/><FRBRlanguage language="de"/></FRBRExpression>
<FRBRManifestation><FRBRthis value="/eli/cc/2000/2/de/xml"/><FRBRformat value="xml"/>
</FRBRManifestation></identification></meta><body>
<proviso eId="disp_u1"><heading>Übergangsbestimmung zur Teständerung</heading>
<paragraph eId="disp_u1/para_2"><num>2</num><content><p>Zweiter Absatz.</p></content></paragraph>
<paragraph eId="disp_u1/para_3"><num>3</num><content><p>Dritter Absatz.</p>
<blockList><item eId="disp_u1/para_3/lbl_a"><num>a.</num><p>Erster Buchstabe.</p></item></blockList>
</content></paragraph></proviso>
<proviso eId="disp_u2"><paragraph eId="disp_u2/para"><content><p>Ohne Überschrift.</p></content></paragraph></proviso>
</body></act></akomaNtoso>"##;
        let doc = AknDocument::parse(xml).expect("the synthetic document parses");
        let as_of = valid_as_of("2026-08-29").expect("a date");
        let label = |eid| transitional_label(&doc, eid, as_of, "de", Some("XYZ"));
        assert_eq!(
            label("disp_u1").unwrap(),
            "Übergangsbestimmung zur Teständerung XYZ"
        );
        // The branch no fixture carries: a NUMBERED paragraph.
        assert_eq!(
            label("disp_u1/para_2").unwrap(),
            "Übergangsbestimmung zur Teständerung Abs. 2 XYZ"
        );
        assert_eq!(
            label("disp_u1/para_3/lbl_a").unwrap(),
            "Übergangsbestimmung zur Teständerung Abs. 3 Bst. a XYZ"
        );
        // And the other: a provision WITHOUT a heading is refused with
        // its true reason.
        let headless = label("disp_u2/para").unwrap_err();
        assert!(
            headless.contains("without a heading") && headless.contains("no citation grammar yet"),
            "{headless}"
        );
        // A segment below it that names nothing is named in the refusal.
        let unknown = label("disp_u1/xyz_1").unwrap_err();
        assert!(unknown.contains("«xyz_1»"), "{unknown}");
        // Without an act the label is the heading and the sub-address.
        assert_eq!(
            transitional_label(&doc, "disp_u1/para_2", as_of, "de", None).unwrap(),
            "Übergangsbestimmung zur Teständerung Abs. 2"
        );
    }

    /// The federal WAF blocks a long query that contains the token
    /// «from» (vendored fedlex-jolux, live-diagnosed): the consultation
    /// queries stay «from»-free, as a word, in any case — the same guard
    /// the vendored crate keeps over its own queries.
    #[test]
    fn the_consultation_queries_stay_from_free_for_the_waf() {
        let has_from = |q: &str| {
            q.split(|c: char| !c.is_alphanumeric())
                .any(|word| word.eq_ignore_ascii_case("from"))
        };
        let draft = "https://fedlex.data.admin.ch/eli/dl/proj/8022/0491";
        let cons = "https://fedlex.data.admin.ch/eli/dl/proj/6016/61/cons_1";
        assert!(!has_from(&consultations_query(draft)));
        assert!(!has_from(&consultation_documents_query(cons)));
        assert!(
            has_from("SELECT * FROM <x>"),
            "the guard itself sees the word"
        );
        assert!(has_from("SELECT ?x WHERE { ?x ?p ?from }"));
        assert!(
            !has_from("SELECT ?x WHERE { ?x ?p ?fromage }"),
            "a word, not a substring"
        );
    }

    /// Every typographic mark the normalisation folds, one by one —
    /// and the two shapes it must NOT fold.
    #[test]
    fn quote_normalisation_folds_every_named_mark() {
        assert_eq!(normalise_quote("‟x‟"), "\"x\"", "U+201F");
        assert_eq!(normalise_quote("‚x‘ ’y’ ‹z›"), "'x' 'y' 'z'");
        assert_eq!(
            normalise_quote("Ver\u{00AD}ord\u{00AD}nung"),
            "Verordnung",
            "soft hyphen"
        );
        assert_eq!(
            normalise_quote("7\u{2011}9"),
            "7-9",
            "no-break hyphen U+2011"
        );
        assert_eq!(normalise_quote("a\u{2014}b"), "a-b", "em dash");
        assert_eq!(
            normalise_quote("K1 = \u{2212}5 neben \u{2013}15"),
            "K1 = -5 neben -15",
            "minus U+2212, en dash"
        );
        // The whitespace fold, which the marks above do not pin: a
        // no-break space, a run of blanks and a line break all become
        // one single space.
        assert_eq!(
            normalise_quote("Anhang\u{00A0}II  der\nVerordnung"),
            "Anhang II der Verordnung",
            "no-break space and folded whitespace"
        );
        // «[ ... ]» with inner spaces is not an omission mark: it stays
        // wording, and the quote comes back verbatim — «[ … ]» is never
        // rewritten to «[ ... ]» (BT′ audit).
        assert_eq!(quote_segments("a [ ... ] b"), vec!["a [ ... ] b"]);
        assert_eq!(quote_segments("a [ … ] b"), vec!["a [ … ] b"]);
        assert_eq!(
            quote_segments("a [ … ] b … c"),
            vec!["a [ … ] b", "c"],
            "the bracketed one is kept, the bare one splits"
        );
        assert_eq!(quote_segments("[ x ] y"), vec!["[ x ] y"]);
        assert_eq!(quote_segments("a [...] b"), vec!["a", "b"]);
        assert_eq!(quote_segments("a […] b"), vec!["a", "b"]);
        // An ellipsis in the norm text itself (a repealed paragraph reads
        // «…») is not quotable: the quote «…» carries no segment.
        assert!(quote_segments("…").is_empty());
        assert_eq!(quote_segments("4 …"), vec!["4"]);
    }

    /// Inserted letters carry a Latin suffix in both directions:
    /// «Bst. fbis» ↔ `lbl_f_bis`, like «Abs. 1bis» ↔ `para_1_bis`.
    #[test]
    fn suffixed_letters_are_read_and_written() {
        assert!(is_letter_token("b"));
        assert!(is_letter_token("fbis"));
        assert!(is_letter_token("gter"));
        assert!(!is_letter_token("Zugabe"));
        assert!(!is_letter_token("b2"));
        assert!(!is_letter_token("bis"), "a bare suffix is not a letter");
        assert!(!is_letter_token("ter"));
        assert_eq!(
            parse_segment("Art. 25 Abs. 2 Bst. bis KVG").letter,
            None,
            "«Bst. bis» names no letter"
        );
        assert_eq!(
            parse_segment("Art. 7 Abs. 1 Buchstabe b LSV")
                .letter
                .as_deref(),
            Some("b"),
            "«Buchstabe b» is read like «Bst. b» and «lit. b»"
        );
        assert!(!is_letter_token("bisbis"));
        assert_eq!(split_latin_suffix("fbis"), ("f", Some("bis")));
        assert_eq!(split_latin_suffix("bis"), ("bis", None));
        let r = parse_segment("Art. 25 Abs. 2 Bst. fbis KVG");
        assert_eq!(r.letter.as_deref(), Some("fbis"));
        assert_eq!(
            eid_candidate(&r).as_deref(),
            Some("art_25/para_2/lbl_f_bis")
        );
        let c = eid_components("art_25/para_2/lbl_f_bis").unwrap();
        assert_eq!(c.letter.as_deref(), Some("fbis"));
        assert_eq!(
            citation_label("de", &c, Some("KVG"), None),
            "Art. 25 Abs. 2 Bst. fbis KVG"
        );
        assert_eq!(
            citation_label(
                "de",
                &eid_components("art_7/para_1/lbl_b").unwrap(),
                Some("LSV"),
                None
            ),
            "Art. 7 Abs. 1 Bst. b LSV"
        );
        // «lit.» is still read, never written.
        assert_eq!(
            parse_segment("Art. 7 Abs. 1 lit. b LSV").letter.as_deref(),
            Some("b")
        );
        assert_eq!(citation_word("de", CitationPart::Letter), "Bst.");
    }

    /// The eId grammar round-trips through both directions of the one
    /// table: what eid_candidate writes, eid_components reads back.
    #[test]
    fn eid_components_invert_the_candidate_grammar() {
        for (eid, article, paragraph, letter, number) in [
            ("art_6", "6", None, None, None),
            ("art_25_a/para_1_bis", "25a", Some("1bis"), None, None),
            ("art_7/para_1/lbl_b", "7", Some("1"), Some("b"), None),
            (
                "art_3/para_1/lbl_c/lbl_2",
                "3",
                Some("1"),
                Some("c"),
                Some("2"),
            ),
            ("art_23_a/para", "23a", None, None, None),
            (
                "art_84_a/para_1/lbl_g_bis",
                "84a",
                Some("1"),
                Some("gbis"),
                None,
            ),
        ] {
            let c = eid_components(eid).unwrap_or_else(|e| panic!("{eid}: {e:?}"));
            assert_eq!(c.article.as_deref(), Some(article), "{eid}");
            assert_eq!(c.paragraph.as_deref(), paragraph, "{eid}");
            assert_eq!(c.letter.as_deref(), letter, "{eid}");
            assert_eq!(c.number.as_deref(), number, "{eid}");
            let back = ParsedReference {
                article: c.article.clone(),
                paragraph: c.paragraph.clone(),
                letter: c.letter.clone(),
                number: c.number.clone(),
                ..Default::default()
            };
            let expected = eid.trim_end_matches("/para");
            assert_eq!(eid_candidate(&back).as_deref(), Some(expected), "{eid}");
        }
        let annex = eid_components("annex_3/lvl_u1/lvl_2").unwrap();
        assert_eq!(annex.annex.as_deref(), Some("3"));
        assert!(annex.article.is_none());
        assert_eq!(eid_components("sec_1").unwrap_err(), EidShape::Structural);
        assert_eq!(
            eid_components("chp_2/sec_1").unwrap_err(),
            EidShape::Structural
        );
        assert_eq!(
            eid_components("preamble").unwrap_err(),
            EidShape::Structural
        );
        assert_eq!(
            eid_components("disp_u1").unwrap_err(),
            EidShape::Transitional
        );
        assert_eq!(
            eid_components("disp_u11/para").unwrap_err(),
            EidShape::Transitional
        );
        // The refusal names the OFFENDING segment, not the first one.
        assert_eq!(
            eid_components("xyz_1").unwrap_err(),
            EidShape::Unknown("xyz_1".into())
        );
        assert_eq!(
            eid_components("art_5/lbl_?").unwrap_err(),
            EidShape::Unknown("lbl_?".into())
        );
        assert_eq!(
            eid_components("art_84_a/para_1/lbl_f_undecies").unwrap_err(),
            EidShape::Unknown("lbl_f_undecies".into()),
            "the third segment is the one without a grammar"
        );
        assert_eq!(
            eid_components("main").unwrap_err(),
            EidShape::Structural,
            "«main» is a structural element like body and preamble"
        );
        assert_eq!(
            citation_label("de", &annex, Some("LSV"), None),
            "Anhang 3 LSV"
        );
        let unnumbered = eid_components("annex_u1/lvl_u1").unwrap();
        assert!(unnumbered.unnumbered_annex);
        assert_eq!(
            citation_label("de", &unnumbered, Some("BGÖ"), None),
            "Anhang BGÖ"
        );
        let c = eid_components("art_7/para_1/lbl_b").unwrap();
        assert_eq!(
            citation_label("fr", &c, Some("OPB"), Some("814.41")),
            "art. 7 al. 1 let. b OPB"
        );
        assert_eq!(
            citation_label("it", &c, None, Some("814.41")),
            "art. 7 cpv. 1 lett. b (SR 814.41)"
        );
        assert_eq!(
            citation_label("de", &c, Some("LSV"), None),
            "Art. 7 Abs. 1 Bst. b LSV"
        );
    }

    /// One vocabulary, both directions: every word cite can write,
    /// parse_reference reads — in every language.
    #[test]
    fn the_vocabulary_is_read_in_every_spelling_it_writes() {
        for v in &VOCABULARY {
            assert_eq!(citation_part(v.article[0]), Some(CitationPart::Article));
            assert_eq!(citation_part(v.paragraph[0]), Some(CitationPart::Paragraph));
            assert_eq!(citation_part(v.letter[0]), Some(CitationPart::Letter));
            assert_eq!(citation_part(v.number[0]), Some(CitationPart::Number));
            assert_eq!(citation_part(v.annex[0]), Some(CitationPart::Annex));
        }
        assert_eq!(citation_part("Abs"), Some(CitationPart::Paragraph));
        assert_eq!(citation_part("BGÖ"), None);
        assert!(is_abbreviation_token("BGÖ"));
        assert!(!is_abbreviation_token("Anhang"));
        let r = parse_segment("art. 7 al. 1 let. b OPB");
        assert_eq!(r.article.as_deref(), Some("7"));
        assert_eq!(r.paragraph.as_deref(), Some("1"));
        assert_eq!(r.letter.as_deref(), Some("b"));
        assert_eq!(r.abbreviation.as_deref(), Some("OPB"));
    }

    fn node(
        eid: &str,
        kind: &str,
        children: Vec<fedlex_akn::OutlineNode>,
    ) -> fedlex_akn::OutlineNode {
        fedlex_akn::OutlineNode {
            eid: Some(eid.into()),
            kind: kind.into(),
            num: None,
            heading: None,
            children,
        }
    }

    /// The outline cap keeps a document-order PREFIX (the first
    /// `budget` nodes, parents before children), never a sample.
    #[test]
    fn outline_cap_is_a_document_order_prefix() {
        let mut tree = vec![
            node(
                "sec_1",
                "section",
                vec![
                    node("art_1", "article", vec![]),
                    node("art_2", "article", vec![]),
                ],
            ),
            node(
                "sec_2",
                "section",
                vec![
                    node("art_3", "article", vec![]),
                    node("art_4", "article", vec![]),
                ],
            ),
        ];
        assert_eq!(count_nodes(&tree), 6);
        let mut budget = 4;
        cap_nodes(&mut tree, &mut budget);
        assert_eq!(count_nodes(&tree), 4);
        assert_eq!(tree.len(), 2, "sec_2 is kept as the fourth node …");
        assert!(tree[1].children.is_empty(), "… without its children");
        assert_eq!(tree[0].children.len(), 2);
    }

    #[test]
    fn depth_article_cuts_below_articles_only() {
        let mut tree = vec![node(
            "sec_1",
            "section",
            vec![node(
                "art_1",
                "article",
                vec![node("art_1/para_1", "paragraph", vec![])],
            )],
        )];
        prune_below_articles(&mut tree);
        assert_eq!(tree[0].children.len(), 1);
        assert!(tree[0].children[0].children.is_empty());
        // The other branch of the same function: a level-based document
        // carries no article, so nothing is cut and the tree survives.
        let mut levels = vec![node(
            "lvl_u1",
            "level",
            vec![node("lvl_u1/lvl_1", "level", vec![])],
        )];
        prune_below_articles(&mut levels);
        assert_eq!(
            count_nodes(&levels),
            2,
            "a level-based document keeps its tree"
        );
    }

    #[test]
    fn eid_gate_accepts_paths_and_refuses_escapes() {
        for ok in [
            "art_1",
            "art_2/para_1",
            "annex_u1/lvl_u1",
            "art_23_a",
            "art_1.1",
        ] {
            assert!(valid_eid(ok).is_ok(), "{ok}");
        }
        for bad in [
            "", "/art_1", "art_1/", "a//b", "../x", "art 1", "<x>", "art_1/..", "a/../b",
        ] {
            assert!(valid_eid(bad).is_err(), "{bad}");
        }
        // The gate is a CHARSET gate, nothing more: a well-formed eId
        // nobody carries passes it. What refuses an address the parser
        // does not know — instead of guessing at it — is the grammar.
        assert!(valid_eid("xyz_1").is_ok());
        assert_eq!(
            eid_components("xyz_1").unwrap_err(),
            EidShape::Unknown("xyz_1".into())
        );
        assert_eq!(
            eid_components("art_5/lbl_?").unwrap_err(),
            EidShape::Unknown("lbl_?".into()),
            "the offending segment is named, not the whole path"
        );
    }

    /// The friendly language parameter is mapped onto the vocabulary
    /// IRI the graph is asked with — all five official languages — and
    /// a language the vocabulary does not define is refused as
    /// invalid-input, before any query leaves this process (J17.4).
    #[test]
    fn manifestation_lang_maps_the_five_onto_their_vocabulary_iris() {
        for (asked, tag, language) in [
            (None, "de", Language::De),
            (Some("de"), "de", Language::De),
            (Some("fr"), "fr", Language::Fr),
            (Some("it"), "it", Language::It),
            (Some("en"), "en", Language::En),
            (Some("rm"), "rm", Language::Roh),
        ] {
            let mapped = manifestation_lang(asked).unwrap();
            assert_eq!(
                mapped,
                (tag, language.vocab_uri()),
                "lang «{asked:?}» is asked as {mapped:?}"
            );
        }
        let refused = manifestation_lang(Some("es")).unwrap_err();
        assert_eq!(refused["error"], "invalid-input", "{refused}");
    }

    /// Foreign-language sections: an element whose xml:lang deviates
    /// from the manifestation language is reported once (its nested
    /// children are folded in), the FRBR names of the meta block are
    /// ignored, and <foreign> islands are classified.
    #[test]
    fn foreign_language_sections_and_islands_are_detected() {
        let xml = r##"<?xml version="1.0" encoding="UTF-8"?>
<akomaNtoso xmlns="http://docs.oasis-open.org/legaldocml/ns/akn/3.0">
  <act name="publicLaw">
    <meta><identification source="#x"><FRBRWork><FRBRthis value="/eli/cc/2000/1"/><FRBRuri value="/eli/cc/2000/1"/>
      <FRBRname xml:lang="fr" value="Loi"/><FRBRname xml:lang="de" value="Gesetz"/></FRBRWork>
      <FRBRExpression><FRBRthis value="/eli/cc/2000/1/de"/><FRBRlanguage language="de"/></FRBRExpression>
      <FRBRManifestation><FRBRthis value="/eli/cc/2000/1/de/xml"/></FRBRManifestation></identification></meta>
    <body>
      <article eId="art_1"><num>Art. 1</num>
        <paragraph eId="art_1/para_1"><content><p>Deutscher Text.</p></content></paragraph>
        <paragraph eId="art_1/para_2" xml:lang="fr"><content><p>Texte <i xml:lang="fr">français</i> cité.</p></content></paragraph>
      </article>
      <article eId="art_2"><num>Art. 2</num>
        <paragraph eId="art_2/para_1"><content><p>Formel: <foreign><math><mrow><mi>P</mi><mo>=</mo><mn>2</mn></mrow></math></foreign></p></content></paragraph>
      </article>
    </body>
  </act>
</akomaNtoso>"##;
        let doc = AknDocument::parse(xml).expect("parses");
        let sections = foreign_language_sections(&doc, "de");
        assert_eq!(sections.len(), 1, "{sections:?}");
        assert_eq!(sections[0]["eid"], "art_1/para_2");
        assert_eq!(sections[0]["lang"], "fr");
        assert!(sections[0]["snippet"]
            .as_str()
            .unwrap()
            .contains("français"));
        let islands = fedlex_akn::detect_foreign_content(&doc);
        assert_eq!(islands.len(), 1);
        assert_eq!(islands[0].context_eid.as_deref(), Some("art_2/para_1"));
        // WHAT the island is, not only that it is there: the classifier
        // reads the local names <math>/<mrow>/<mi>, which carry no
        // MathML namespace here, and names a formula — not a graphic.
        assert_eq!(
            islands[0].kind,
            fedlex_akn::ForeignKind::MathMl,
            "a formula, named as one by its local names: {:?}",
            islands[0]
        );
        // And the formula stays an island: its body is never flattened
        // into the text of the paragraph that carries it.
        let formula = fedlex_akn::resolve_eid(&doc, "art_2/para_1").expect("art_2/para_1 resolves");
        let around = doc.text_of(formula.node);
        assert_eq!(
            around, "Formel:",
            "the formula body stays out of the text: {around}"
        );
    }

    /// The citation parser on its own: spellings, paths, separators.
    #[test]
    fn citation_parser_reads_every_spelling() {
        let r = parse_segment("Art. 7 Abs. 1 lit. b LSV");
        assert_eq!(r.article.as_deref(), Some("7"));
        assert_eq!(r.paragraph.as_deref(), Some("1"));
        assert_eq!(r.letter.as_deref(), Some("b"));
        assert_eq!(r.abbreviation.as_deref(), Some("LSV"));
        assert_eq!(eid_candidate(&r).as_deref(), Some("art_7/para_1/lbl_b"));
        let r = parse_segment("Art. 25a Abs. 1bis KVG");
        assert_eq!(eid_candidate(&r).as_deref(), Some("art_25_a/para_1_bis"));
        let r = parse_segment("Art. 3 Abs. 1 Bst. c Ziff. 2 DSG");
        assert_eq!(
            eid_candidate(&r).as_deref(),
            Some("art_3/para_1/lbl_c/lbl_2")
        );
        let r = parse_segment("Anhang 3 Ziff. 2 LSV");
        assert_eq!(r.kind, "annex");
        assert_eq!(r.annex.as_deref(), Some("3"));
        assert_eq!(r.number.as_deref(), Some("2"));
        assert!(eid_candidate(&r).is_none());
        let r = parse_segment("Art. 41 ff. OR");
        assert!(r.following);
        assert_eq!(r.abbreviation.as_deref(), Some("OR"));
        let r = parse_segment("ArGV 1 Art. 13");
        assert_eq!(r.abbreviation.as_deref(), Some("ArGV 1"));
        let r = parse_segment("AS 2020 752");
        assert_eq!(r.kind, "as");
        assert_eq!(r.memorial.as_deref(), Some("AS 2020 752"));
        assert_eq!(
            split_references("Art. 8 EMRK i.V.m. Art. 36 BV; Anhang 1 LSV"),
            vec!["Art. 8 EMRK", "Art. 36 BV", "Anhang 1 LSV"]
        );
    }

    #[test]
    fn scope_matches_the_element_and_its_descendants_only() {
        assert!(within_eid(Some("art_2"), "art_2"));
        assert!(within_eid(Some("art_2/para_1"), "art_2"));
        assert!(!within_eid(Some("art_20"), "art_2"));
        assert!(!within_eid(None, "art_2"));
    }

    /// A synthetic document, small enough to read at a glance, for the
    /// three BV A′ rules that need a shape the recorded corpus does not
    /// carry: a second header row, a body-level paragraph BETWEEN two
    /// articles, an annex whose first level is not `lvl_u1`, and a
    /// line break written as `<br/>`.
    fn synthetic_document() -> AknDocument {
        let xml = r##"<?xml version="1.0" encoding="UTF-8"?>
<akomaNtoso xmlns="http://docs.oasis-open.org/legaldocml/ns/akn/3.0">
  <act name="publicLaw">
    <meta><identification source="#x"><FRBRWork><FRBRthis value="/eli/cc/2000/1"/><FRBRuri value="/eli/cc/2000/1"/>
      <FRBRname xml:lang="de" value="Gesetz"/></FRBRWork>
      <FRBRExpression><FRBRthis value="/eli/cc/2000/1/de"/><FRBRlanguage language="de"/></FRBRExpression>
      <FRBRManifestation><FRBRthis value="/eli/cc/2000/1/de/xml"/></FRBRManifestation></identification></meta>
    <body>
      <article eId="art_1"><num>Art. 1</num>
        <paragraph eId="art_1/para_1"><content>
          <p>Der Wert<br/>ist gross.</p>
          <table>
            <tr><th>Stufe</th><th>Grenzwert</th><th>Grenzwert</th></tr>
            <tr><th></th><th>Tag</th><th>Nacht</th></tr>
            <tr><td>I</td><td>50</td><td>40</td></tr>
          </table>
        </content></paragraph>
      </article>
      <p>Ein Satz zwischen den Artikeln.</p>
      <article eId="art_2"><num>Art. 2</num>
        <paragraph eId="art_2/para_1"><content><p>Zweiter Text.</p></content></paragraph>
      </article>
      <level eId="annex_1/lvl_1"><content><p>Erstes Level.</p></content></level>
      <level eId="annex_1/lvl_u1"><content><p>Zweites Level.</p></content></level>
      <signature>Im Namen des Bundesrates</signature>
    </body>
  </act>
</akomaNtoso>"##;
        AknDocument::parse(xml).expect("parses")
    }

    /// X6.3 — a row of `<th>` cells after row 0 is a SECOND header the
    /// corpus writes (column groups: «Tag | Nacht» under «Grenzwert»).
    /// The vendored extractor dropped it with everything in it; here it
    /// is kept as a data row and named, so a reader can tell it from a
    /// measurement.
    #[test]
    fn a_second_header_row_is_kept_as_a_data_row_and_named() {
        let doc = synthetic_document();
        let table = doc
            .find_all(doc.root(), "table")
            .into_iter()
            .next()
            .expect("the synthetic table");
        let out = table_json(&doc, table);
        assert_eq!(out["header"], json!(["Stufe", "Grenzwert", "Grenzwert"]));
        assert_eq!(out["header_inferred"], false, "row 0 carries <th> cells");
        assert_eq!(out["cols"], 3);
        assert_eq!(out["rows"], 3, "the header row counts as a row");
        let data = out["data"].as_array().expect("data");
        assert_eq!(data.len(), 2, "the th-only row is NOT dropped: {out}");
        assert_eq!(data[0], json!(["", "Tag", "Nacht"]));
        assert_eq!(data[1], json!(["I", "50", "40"]));
        assert_eq!(out["sub_header_rows"], json!([0]), "and it is named as one");
    }

    /// X18.7 — text that sits directly under `<body>` among the
    /// hierarchy is rendered AT ITS POSITION, not prepended: the
    /// sentence between Art. 1 and Art. 2 stands between them, the
    /// signature after the last article.
    #[test]
    fn body_level_text_is_rendered_at_its_place_in_document_order() {
        let doc = synthetic_document();
        let extras = body_level_elements(&doc);
        let tags: Vec<&str> = extras.iter().map(|e| e.tag.as_str()).collect();
        assert_eq!(tags, vec!["p", "signature"], "{extras:?}");
        assert!(!extras[0].after_hierarchy, "the sentence is among them");
        assert!(extras[1].after_hierarchy, "the signature is after them");
        assert_eq!(
            extras[0].anchor.as_deref(),
            Some("Art. 2"),
            "the line the next hierarchy sibling opens with"
        );
        let (markdown, _) =
            fedlex_akn::get_readable_document(&doc, valid_as_of("2026-08-29").expect("a date"))
                .expect("renders")
                .into_parts();
        assert!(
            !markdown.contains("Ein Satz zwischen den Artikeln."),
            "the vendored renderer walks the hierarchy only — that IS the gap"
        );
        let rendered = render_body_level(markdown, &extras);
        let at = |needle: &str| {
            rendered
                .find(needle)
                .unwrap_or_else(|| panic!("«{needle}» missing from:\n{rendered}"))
        };
        assert!(at("Der Wert") < at("Ein Satz zwischen den Artikeln."));
        assert!(at("Ein Satz zwischen den Artikeln.") < at("Art. 2"));
        assert!(at("Zweiter Text.") < at("Im Namen des Bundesrates"));
    }

    /// The first level under an annex wrapper is READ in document
    /// order, never assumed to be `lvl_u1` — here `lvl_1` stands first.
    #[test]
    fn the_first_level_of_an_annex_is_read_not_assumed() {
        let doc = synthetic_document();
        assert_eq!(
            first_level_of(&doc, "annex_1").as_deref(),
            Some("annex_1/lvl_1")
        );
        assert_eq!(first_level_of(&doc, "annex_9").as_deref(), None);
    }

    /// X6.1 — a line break written as `<br/>` leaves NO separator in
    /// the extracted text: «Der Wert<br/>ist gross.» comes out as «Der
    /// Wertist gross.», so a quote copied from the rendered page (with
    /// the break as a space) does not match. The rule is not honoured;
    /// this test holds the consequence still.
    #[test]
    fn a_line_break_leaves_no_separator_in_the_extracted_text() {
        let doc = synthetic_document();
        let article = doc
            .find_all(doc.root(), "article")
            .into_iter()
            .next()
            .expect("art_1");
        let text = doc.text_of(article);
        assert!(text.contains("Wertist"), "the break vanishes: {text}");
        let haystack = normalise_quote(&text);
        let quote = normalise_quote("Der Wert ist gross.");
        assert!(
            !haystack.contains(&quote),
            "a quote spanning the break cannot be verified: {haystack}"
        );
        assert!(
            haystack.contains(&normalise_quote("Der Wertist gross.")),
            "what the tools DO carry: {haystack}"
        );
    }

    /// J3.2 — an act may carry TWO end dates, and they disagree on about
    /// 4 % of expired acts. The rule reads both, the EARLIER one decides,
    /// and the answer names the field that decided.
    #[test]
    fn the_in_force_rule_names_the_field_that_decided() {
        // Both end dates, the earlier one deciding — either way round.
        assert_eq!(
            in_force_reason(
                "2026-08-29",
                Some("1999-01-01"),
                Some("2018-01-01"),
                Some("2020-01-01"),
                None
            ),
            (false, "no_longer_in_force")
        );
        assert_eq!(
            in_force_reason(
                "2026-08-29",
                Some("1999-01-01"),
                Some("2020-01-01"),
                Some("2018-01-01"),
                None
            ),
            (false, "end_applicability")
        );
        // An end date that has NOT passed decides nothing.
        assert_eq!(
            in_force_reason(
                "2017-12-31",
                Some("1999-01-01"),
                Some("2018-01-01"),
                None,
                None
            ),
            (true, "entry_in_force")
        );
        // A date in the future: not yet in force, and the answer says
        // which field it is waiting for.
        assert_eq!(
            in_force_reason("2026-08-29", Some("2030-01-01"), None, None, None),
            (false, "entry_in_force (not yet reached)")
        );
        // No date at all: the status vocabulary decides, and says so.
        assert_eq!(
            in_force_reason("2026-08-29", None, None, None, Some(IN_FORCE_STATUS)),
            (true, "status (no date in the graph)")
        );
        assert_eq!(
            in_force_reason("2026-08-29", None, None, None, None),
            (false, "nothing — the graph carries neither date nor status")
        );
    }

    /// X16.4 — most tables are small, a few are enormous (the largest in
    /// the corpus has 5'308 rows). The cap must be MEASURED and
    /// reported, never silently applied: no recorded table is large
    /// enough to make it bite, so a synthetic one is.
    #[test]
    fn the_table_row_cap_is_measured_and_reported() {
        let mut xml = String::from(
            r##"<?xml version="1.0" encoding="UTF-8"?>
<akomaNtoso xmlns="http://docs.oasis-open.org/legaldocml/ns/akn/3.0"><act name="publicLaw">
  <meta><identification source="#x"><FRBRWork><FRBRthis value="/eli/cc/2000/1"/><FRBRuri value="/eli/cc/2000/1"/>
    <FRBRname xml:lang="de" value="Gesetz"/></FRBRWork>
    <FRBRExpression><FRBRthis value="/eli/cc/2000/1/de"/><FRBRlanguage language="de"/></FRBRExpression>
    <FRBRManifestation><FRBRthis value="/eli/cc/2000/1/de/xml"/></FRBRManifestation></identification></meta>
  <body><article eId="art_1"><num>Art. 1</num><paragraph eId="art_1/para_1"><content><table>
    <tr><th>Nummer</th><th>Wert</th></tr>"##,
        );
        for row in 1..=250 {
            xml.push_str(&format!("<tr><td>{row}</td><td>{}</td></tr>", row * 2));
        }
        xml.push_str("</table></content></paragraph></article></body></act></akomaNtoso>");
        let doc = AknDocument::parse(&xml).expect("parses");
        let node = doc
            .find_all(doc.root(), "table")
            .into_iter()
            .next()
            .expect("the table");
        let out = table_json(&doc, node);
        assert_eq!(out["rows"], 251, "the header row counts too");
        assert_eq!(out["rows_total"], 250, "the original size stays visible");
        assert_eq!(out["rows_returned"], MAX_TABLE_ROWS, "the cap bites");
        assert_eq!(out["truncated"], true, "and says so: {out}");
        assert_eq!(out["data"].as_array().unwrap().len(), MAX_TABLE_ROWS);
        assert_eq!(out["oversized"], true, "more than 100 rows");
    }

    /// X11.2 — fifteen per cent of the corpus's references carry no
    /// target. A reference without an href is still a reference: it is
    /// kept with a null href and counted, never dropped to make the
    /// answer look complete. No recorded manifestation carries one, so
    /// a synthetic document holds the case.
    #[test]
    fn a_reference_without_a_target_is_kept_and_counted() {
        let xml = r##"<?xml version="1.0" encoding="UTF-8"?>
<akomaNtoso xmlns="http://docs.oasis-open.org/legaldocml/ns/akn/3.0"><act name="publicLaw">
  <meta><identification source="#x"><FRBRWork><FRBRthis value="/eli/cc/2000/1"/><FRBRuri value="/eli/cc/2000/1"/>
    <FRBRname xml:lang="de" value="Gesetz"/></FRBRWork>
    <FRBRExpression><FRBRthis value="/eli/cc/2000/1/de"/><FRBRlanguage language="de"/></FRBRExpression>
    <FRBRManifestation><FRBRthis value="/eli/cc/2000/1/de/xml"/></FRBRManifestation></identification></meta>
  <body><article eId="art_1"><num>Art. 1</num><paragraph eId="art_1/para_1"><content>
    <p>Nach <ref href="https://fedlex.data.admin.ch/eli/cc/1999/404">Artikel 5 BV</ref>
       und nach <ref>dem Übereinkommen</ref>.</p>
  </content></paragraph></article></body></act></akomaNtoso>"##;
        let doc = AknDocument::parse(xml).expect("parses");
        let (refs, _) =
            fedlex_akn::get_all_references(&doc, valid_as_of("2026-08-29").expect("a date"))
                .expect("references")
                .into_parts();
        assert_eq!(refs.len(), 2, "both references are read: {refs:?}");
        assert_eq!(refs.iter().filter(|r| unlinked_reference(r)).count(), 1);
        let rows: Vec<Value> = refs.iter().map(reference_json).collect();
        let unlinked = rows
            .iter()
            .find(|r| r["href"].is_null())
            .expect("the reference without a target is in the answer");
        assert_eq!(unlinked["label"], "dem Übereinkommen");
        assert_eq!(unlinked["source_eid"], "art_1/para_1");
    }

    /// J5.3/J5.4 — which label answers, and in which language: the
    /// caller's first, then de → en → fr → it → rm, and where the graph
    /// has none of them the label it does have, with its tag.
    #[test]
    fn a_label_is_chosen_in_the_language_the_graph_actually_has() {
        let l = |lang: &str, label: &str| (lang.to_string(), label.to_string());
        let french_only = [l("fr", "Code")];
        assert_eq!(
            choose_label(&french_only, Some("de")),
            Some(("Code".into(), "fr".into())),
            "no German label is a real case, not an empty answer"
        );
        let both = [l("fr", "Code"), l("de", "Kodex")];
        assert_eq!(choose_label(&both, Some("de")).unwrap().1, "de");
        assert_eq!(
            choose_label(&both, Some("it")).unwrap().1,
            "de",
            "the caller's language first, then the fallback order"
        );
        assert_eq!(
            choose_label(&[l("roh", "Cudesch")], Some("rm")).unwrap().1,
            "roh",
            "Romansh is written «rm» and «roh»"
        );
        assert_eq!(choose_label(&[], Some("de")), None);
        assert_eq!(
            choose_label(&[l("", "Ohne Sprache")], Some("de")),
            Some(("Ohne Sprache".into(), "und".into())),
            "an untagged label is answered, and says it has no language"
        );
    }
}
