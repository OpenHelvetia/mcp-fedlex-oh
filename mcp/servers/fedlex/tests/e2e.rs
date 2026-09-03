//! Conformance suite for the fedlex base-tier server: everything
//! runs OFFLINE on recorded fixtures (single-request live recording
//! is a deliberate `--ignored` pass; a second `--ignored` smoke
//! proves liveness end to end). The e2e path is real stdio sessions
//! against the built binary — the gateway-proven harness pattern.
//!
//! BQ: the XML tools of the navigator surface run on the BGÖ
//! manifestation fixture (SR 152.3, version 20231101, de) — one
//! recorded reality, seven tools, no live request; the JOLux tools
//! run on their own recorded keys (one polite request each at the
//! recording pass).

use std::io::Write as _;
use std::process::{Command, Stdio};

use oh_mcp_fedlex::backend::{
    Backend, FrozenClock, ManifestationCache, UpstreamThrottle, FEDLEX_ENDPOINT,
};
use oh_mcp_fedlex::domain::{self, Ctx};
use oh_mcp_fedlex::server::{summary_conforms, FedlexServer};

const KVG: &str = "https://fedlex.data.admin.ch/eli/cc/1995/1328_1328_1328";
/// BGÖ (SR 152.3) — the compact fixture act for the XML tools.
const BGOE: &str = "https://fedlex.data.admin.ch/eli/cc/2006/355";
const BGOE_VERSION: &str = "https://fedlex.data.admin.ch/eli/cc/2006/355/20231101";
/// LSV — Lärmschutz-Verordnung (SR 814.41): annexes with tables, the
/// second XML reality recorded at BQ.
const LSV_SR: &str = "814.41";
/// An ELI the graph does not know (recorded as such at build).
const UNKNOWN: &str = "https://fedlex.data.admin.ch/eli/cc/1900/000_000_000";
/// StPO — the abbreviation proof: `jolux:titleShort` → `cc/2010/267`.
const STRAFPROZESSORDNUNG: &str = "https://fedlex.data.admin.ch/eli/cc/2010/267";
/// nDSG — «datenschutz» must rank the act in force first.
const NDSG: &str = "https://fedlex.data.admin.ch/eli/cc/2022/491";
/// EMRK (SR 0.101) — the treaty reality, recorded at BO′.
const EMRK_SR: &str = "0.101";
const STATELESS_META: &str = r#"{"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientInfo":{"name":"fedlex-e2e","version":"0.0.0"},"io.modelcontextprotocol/clientCapabilities":{}}"#;

fn fixtures_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn fixture_ctx() -> Ctx {
    Ctx {
        backend: Backend::Fixtures {
            dir: fixtures_dir(),
        },
        today: "2026-08-21".into(),
    }
}

/// The recorded manifestation index: `<file> <key> <recorded>` lines,
/// notes beginning with `#`.
fn recorded_keys() -> Vec<(String, String, String)> {
    std::fs::read_to_string(fixtures_dir().join("INDEX.txt"))
        .expect("INDEX.txt")
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .map(|l| {
            // `<file> <key> <recorded>` — and a KEY may carry spaces
            // (a search query is one), so it is what lies between the
            // first token and the last.
            let file = l.split(' ').next().unwrap_or_default().to_string();
            let recorded = l.rsplit(' ').next().unwrap_or_default().to_string();
            let key = oh_mcp_common::fixtures::key_of(l)
                .unwrap_or_default()
                .to_string();
            (file, key, recorded)
        })
        .collect()
}

/// A recorded version IRI for a given act (the BQ recording pass
/// wrote `manifestation:xml:<version>:de` keys for KVG and LSV).
fn recorded_version_for(abstract_prefix: &str) -> Option<String> {
    recorded_keys()
        .into_iter()
        .filter_map(|(_, key, _)| key.strip_prefix("manifestation:xml:").map(str::to_string))
        .filter_map(|rest| rest.rsplit_once(':').map(|(v, _)| v.to_string()))
        .find(|v| v.starts_with(abstract_prefix))
}

// --- recording pass (deliberate, single polite requests) --------------

/// Re-records every fixture from the live endpoint. Run explicitly:
/// `cargo test --test e2e record_fixtures -- --ignored --nocapture --test-threads 1`
///
/// One polite request per fixture key, strictly sequential — run ONE
/// recording test per invocation, never several side by side; the BQ
/// additions (JOLux keys, the KVG and LSV manifestations) follow the
/// v0 set.
#[test]
#[ignore = "hits the live endpoint once per fixture; run deliberately"]
fn record_fixtures() {
    let ctx = Ctx {
        backend: Backend::recording(FEDLEX_ENDPOINT, fixtures_dir()),
        today: "2026-08-21".into(),
    };
    domain::resolve_sr(&ctx, "832.10").expect("record");
    domain::resolve_sr(&ctx, "999.99").expect("record");
    domain::search_law(&ctx, "krankenversicherung", Some(5)).expect("record");
    domain::get_law_metadata(&ctx, KVG, None).expect("record");
    domain::get_law_metadata(
        &ctx,
        "https://fedlex.data.admin.ch/eli/cc/1900/000_000_000",
        None,
    )
    .expect("record");
    domain::list_versions(&ctx, KVG).expect("record");
    domain::get_citations(&ctx, KVG, "in").expect("record");
    domain::get_citations(&ctx, KVG, "out").expect("record");
    // read_article: the BGÖ (SR 152.3, a compact act) — governing
    // version, manifestation + XML recorded.
    let bgoe_versions = domain::list_versions(&ctx, BGOE).expect("record");
    assert!(bgoe_versions.get("versions").is_some());
    let governing = domain::resolve_consolidation_at(&ctx, BGOE, "2026-08-21").expect("record");
    let version = governing["eli_version"]
        .as_str()
        .expect("governing version")
        .to_string();
    println!("recording read_article against {version}");
    let article = domain::read_article(&ctx, &version, "art_6", Some("de")).expect("record");
    assert!(
        article.get("error").is_none(),
        "recording must capture a real article: {article}"
    );

    record_fixtures_bq_with(&ctx, &version);
}

/// Records ONLY the BQ additions (the v0 fixtures stay as recorded at
/// build): `cargo test --test e2e record_fixtures_bq -- --ignored --nocapture --test-threads 1`
#[test]
#[ignore = "hits the live endpoint once per fixture; run deliberately"]
fn record_fixtures_bq() {
    let ctx = Ctx {
        backend: Backend::recording(FEDLEX_ENDPOINT, fixtures_dir()),
        today: "2026-08-21".into(),
    };
    record_fixtures_bq_with(&ctx, BGOE_VERSION);
}

/// Re-records ONLY the two taxonomy keys (the ancestor variable of
/// the query was renamed to `?node` for the spelling gate after the
/// BQ pass; the recorded bindings carry the variable name, so the
/// honest path is two more polite requests, not a hand-edited
/// fixture):
/// `cargo test --test e2e record_fixtures_taxonomy -- --ignored --test-threads 1`
#[test]
#[ignore = "hits the live endpoint twice; run deliberately"]
fn record_fixtures_taxonomy() {
    let ctx = Ctx {
        backend: Backend::recording(FEDLEX_ENDPOINT, fixtures_dir()),
        today: "2026-08-21".into(),
    };
    domain::get_taxonomy(&ctx, KVG).expect("record");
    domain::get_taxonomy(&ctx, BGOE).expect("record");
}

/// Records the keys the BQ REVIEW changed (exact-match article
/// history for Art. 2 and Art. 17 BGÖ, the unknown act for
/// get_subdivisions/get_taxonomy, the cap+1 queries, the 1996 KVG
/// manifestation lookup): `cargo test --test e2e record_fixtures_review -- --ignored --test-threads 1`
#[test]
#[ignore = "hits the live endpoint once per key; run deliberately"]
fn record_fixtures_review() {
    let ctx = Ctx {
        backend: Backend::recording(FEDLEX_ENDPOINT, fixtures_dir()),
        today: "2026-08-21".into(),
    };
    record_fixtures_review_with(&ctx);
}

fn record_fixtures_review_with(ctx: &Ctx) {
    domain::get_article_history(ctx, BGOE, "art_2").expect("record");
    domain::get_article_history(ctx, BGOE, "art_17").expect("record");
    domain::get_subdivisions(ctx, UNKNOWN).expect("record");
    domain::get_taxonomy(ctx, UNKNOWN).expect("record");
    domain::get_taxonomy(ctx, KVG).expect("record");
    domain::get_taxonomy(ctx, BGOE).expect("record");
    domain::list_expressions(ctx, BGOE_VERSION).expect("record");
    domain::list_expressions(ctx, &format!("{KVG}/19960101")).expect("record");
    domain::resolve_vocabulary_label(ctx, "enforcement-status", "kraft", Some("de"))
        .expect("record");
    // The 1996 KVG: the manifestation lookup itself (empty), so the
    // text tools' not-found is recorded reality, not a missing fixture.
    domain::get_structure(ctx, &format!("{KVG}/19960101"), None, None).expect("record");
}

/// Records the BO′ keys (search_law with the abbreviation pre-query
/// and the ranked window, the EMRK chain, the BGÖ French
/// manifestation): `cargo test --test e2e record_fixtures_bo_prime -- --ignored --nocapture --test-threads 1`
#[test]
#[ignore = "hits the live endpoint once per key; run deliberately"]
fn record_fixtures_bo_prime() {
    let ctx = Ctx {
        backend: Backend::recording(FEDLEX_ENDPOINT, fixtures_dir()),
        today: "2026-08-21".into(),
    };
    record_fixtures_bo_prime_with(&ctx);
}

/// Re-records only the search_law TITLE windows (their query changed
/// after the first BO′ recording: window ≥ 40 acts):
/// `cargo test --test e2e record_fixtures_search_windows -- --ignored --nocapture --test-threads 1`
///
/// **Cost: eight live requests, not five** (BY′ counted them). Five
/// title windows, and THREE of these queries are short enough to be
/// abbreviations — «StPO», «OR» and «datenschutz» all pass
/// `looks_like_abbreviation` (≤ 12 characters, ≤ 2 words) — so each of
/// those also sends its `search_law:abbreviation:<query>` pre-query and
/// re-records that key too. 5 + 3 = 8.
///
/// The two multi-word windows of BY point 0 are NOT in this list (BY′
/// point 9): they have their own recorder below, because the argument
/// that the word-wise filter changed nothing for a one-word query
/// rests on these answers being the ones recorded before it — and an
/// obvious-looking recorder that quietly re-records them would spend
/// that argument.
#[test]
#[ignore = "hits the live endpoint once per key; run deliberately"]
fn record_fixtures_search_windows() {
    let ctx = Ctx {
        backend: Backend::recording(FEDLEX_ENDPOINT, fixtures_dir()),
        today: "2026-08-21".into(),
    };
    for (query, limit) in [
        ("StPO", None),
        ("OR", None),
        ("krankenversicherung", Some(5)),
        ("datenschutz", Some(5)),
        ("Quantencomputergesetz", None),
    ] {
        let out = domain::search_law(&ctx, query, limit).expect("record");
        println!("search_law {query}: first {}", out["hits"][0]["eli"]);
    }
}

/// Records ONLY the two multi-word windows of BY point 0 — the five
/// one-word windows above are left alone deliberately: their SPARQL is
/// the same QUESTION under the word-wise filter (one word is one
/// `CONTAINS`, and the fragment is byte-for-byte the old one), so
/// re-recording them would spend five requests to write the same
/// answers, and would silently fold any drift of the
/// live graph into a commit that is about a filter.
///
/// **Cost: two live requests**, one per key — neither query triggers
/// the abbreviation pre-query (both are longer than twelve
/// characters). Budget named in advance at BY point 0: at most 4, with
/// a margin for one retry. It re-records NOTHING else (BY′ point 9).
/// `cargo test --test e2e record_fixtures_bpr -- --ignored --nocapture --test-threads 1`
#[test]
#[ignore = "hits the live endpoint once per key; run deliberately"]
fn record_fixtures_bpr() {
    let ctx = Ctx {
        backend: Backend::recording(FEDLEX_ENDPOINT, fixtures_dir()),
        today: "2026-08-30".into(),
    };
    for query in [
        "Bundesgesetz über die politischen Rechte",
        "politische Rechte",
    ] {
        let out = domain::search_law(&ctx, query, None).expect("record");
        println!(
            "search_law «{query}»: abbreviation_tried={} first={} sr={} of {} hits",
            out["abbreviation_tried"],
            out["hits"][0]["eli"],
            out["hits"][0]["sr"],
            out["hits"].as_array().map(Vec::len).unwrap_or(0)
        );
    }
}

fn record_fixtures_bo_prime_with(ctx: &Ctx) {
    // search_law: abbreviations (pre-query + title window) and the two
    // «before» cases of the finding.
    for (query, limit) in [
        ("StPO", None),
        ("OR", None),
        ("krankenversicherung", Some(5)),
        ("datenschutz", Some(5)),
        ("Quantencomputergesetz", None),
    ] {
        let out = domain::search_law(ctx, query, limit).expect("record");
        println!(
            "search_law {query}: {} hits, first {}",
            out["returned"], out["hits"][0]["eli"]
        );
    }
    // EMRK: the treaty chain, whatever the graph carries — recorded
    // exactly as answered.
    let emrk = domain::resolve_sr(ctx, EMRK_SR).expect("record");
    println!("EMRK = {} ({})", emrk["eli"], emrk["title"]["de"]);
    let eli = emrk["eli"].as_str().expect("EMRK eli").to_string();
    domain::list_versions(ctx, &eli).expect("record");
    let governing = domain::resolve_consolidation_at(ctx, &eli, "2026-08-29").expect("record");
    let version = governing["eli_version"]
        .as_str()
        .expect("EMRK governing version")
        .to_string();
    println!("EMRK governing at 2026-08-29: {version}");
    let expressions = domain::list_expressions(ctx, &version).expect("record");
    println!(
        "EMRK expressions: xml_available={} pdf_only={} none={}",
        expressions["xml_available"],
        expressions["pdf_only"],
        expressions["no_manifestation_listed"]
    );
    let article = domain::read_article(ctx, &version, "art_3", Some("de")).expect("record");
    println!(
        "EMRK art_3 (de): {}",
        article.get("error").unwrap_or(&article["kind"])
    );
    // The BGÖ in French: a second language is a second cache key.
    let fr = domain::read_article(ctx, BGOE_VERSION, "art_6", Some("fr")).expect("record");
    assert!(fr.get("error").is_none(), "{fr}");
}

fn record_fixtures_bq_with(ctx: &Ctx, version: &str) {
    // ---- BQ: the JOLux keys (one polite request each; article
    // history asks twice by upstream design) ----
    let subs = domain::get_subdivisions(ctx, BGOE).expect("record");
    println!("BGÖ subdivisions: {}", subs["total"]);
    domain::get_article_history(ctx, BGOE, "art_2").expect("record");
    domain::get_taxonomy(ctx, KVG).expect("record");
    domain::get_taxonomy(ctx, BGOE).expect("record");
    domain::list_expressions(ctx, version).expect("record");
    // An early KVG consolidation: the PDF-only reality, recorded.
    domain::list_expressions(ctx, &format!("{KVG}/19960101")).expect("record");
    domain::resolve_vocabulary_label(ctx, "enforcement-status", "kraft", Some("de"))
        .expect("record");
    domain::resolve_vocabulary_label(
        ctx,
        "enforcement-status",
        "https://fedlex.data.admin.ch/vocabulary/enforcement-status/0",
        Some("de"),
    )
    .expect("record");
    domain::find_related_topic(ctx, Some(BGOE), None, Some(10)).expect("record");

    // ---- BQ: two more XML realities — the KVG (a large act) and the
    // LSV (annexes with tables) at their governing versions ----
    let kvg_governing = domain::resolve_consolidation_at(ctx, KVG, "2026-08-21").expect("record");
    let kvg_version = kvg_governing["eli_version"]
        .as_str()
        .expect("kvg")
        .to_string();
    println!("recording KVG manifestation {kvg_version}");
    let kvg_structure = domain::get_structure(ctx, &kvg_version, None, None).expect("record");
    assert!(kvg_structure.get("error").is_none(), "{kvg_structure}");
    let lsv = domain::resolve_sr(ctx, LSV_SR).expect("record");
    let lsv_eli = lsv["eli"].as_str().expect("LSV eli").to_string();
    println!("LSV = {lsv_eli} ({})", lsv["title"]["de"]);
    domain::list_versions(ctx, &lsv_eli).expect("record");
    let lsv_governing =
        domain::resolve_consolidation_at(ctx, &lsv_eli, "2026-08-21").expect("record");
    let lsv_version = lsv_governing["eli_version"]
        .as_str()
        .expect("lsv")
        .to_string();
    println!("recording LSV manifestation {lsv_version}");
    let annexes = domain::list_annexes(ctx, &lsv_version, None).expect("record");
    assert!(annexes.get("error").is_none(), "{annexes}");
    record_fixtures_review_with(ctx);
    record_fixtures_bo_prime_with(ctx);
    record_fixtures_br_with(ctx);
    record_fixtures_br_eng_with(ctx);
}

// --- domain semantics on fixtures -------------------------------------

#[test]
fn resolve_sr_disambiguates_to_the_in_force_act_and_keeps_predecessors_visible() {
    let out = domain::resolve_sr(&fixture_ctx(), "832.10").expect("runs");
    // Real-data case: 832.10 matches the 1994 KVG AND its repealed
    // predecessor — the in-force act wins, the predecessor stays
    // visible (E14 predecessor thinking).
    assert_eq!(out["eli"], KVG);
    assert!(out["title"]["de"]
        .as_str()
        .unwrap()
        .contains("Krankenversicherung"));
    assert!(out["title"]["fr"]
        .as_str()
        .unwrap()
        .contains("assurance-maladie"));
    assert_eq!(out["kind"], "norm");
    assert_eq!(out["sr"], "832.10");
    let also = out["also_matches"].as_array().expect("predecessor visible");
    assert!(also
        .iter()
        .any(|m| m["eli"].as_str().unwrap().contains("/cc/28/")));
    assert_eq!(out["provenance"]["transaction_time"], "2026-08-21");
}

#[test]
fn unknown_sr_is_an_honest_not_found_echoing_its_subject() {
    let out = domain::resolve_sr(&fixture_ctx(), "999.99").expect("runs");
    assert_eq!(out["error"], "not-found");
    assert_eq!(out["subject"], "999.99");
}

#[test]
fn malformed_sr_is_invalid_input_without_any_query() {
    // No fixture exists for this key — reaching the backend would
    // fail loudly; a clean refusal proves the gate is BEFORE it.
    let out = domain::resolve_sr(&fixture_ctx(), "DROP GRAPH").expect("runs");
    assert_eq!(out["error"], "invalid-input");
}

#[test]
fn search_is_a_hint_never_a_norm() {
    let out = domain::search_law(&fixture_ctx(), "krankenversicherung", Some(5)).expect("runs");
    assert_eq!(out["kind"], "hint");
    assert!(!out["hits"].as_array().unwrap().is_empty());
    // J9.4: the served scope is the classified compilation — the query
    // walks consolidated acts only, so no Federal-Gazette document can
    // ever appear among the hints.
    assert!(
        out["hits"].as_array().unwrap().iter().all(|h| h["eli"]
            .as_str()
            .unwrap()
            .starts_with("https://fedlex.data.admin.ch/eli/cc/")),
        "the served scope is the classified compilation — never a Federal-Gazette document: {out}"
    );
}

/// BO′, the finding that opened the package: v0's window for
/// «krankenversicherung» held five repealed ordinances of 1965–1987
/// (cc/1965/31_32_33, cc/1966/499_519_515, cc/1987/86_86_86,
/// cc/1986/87_87_87, cc/1965/90_94_93) and not the KVG. Ranked in
/// force first, newest first, the KVG stands first.
#[test]
fn search_law_ranks_the_act_in_force_first() {
    let ctx = fixture_ctx();
    let out = domain::search_law(&ctx, "krankenversicherung", Some(5)).expect("runs");
    assert_eq!(out["kind"], "hint", "{out}");
    let hits = out["hits"].as_array().expect("hits");
    assert_eq!(hits.len(), 5);
    // J15.1: the window keeps the collection's own order — the act, the
    // supervision act, then the ordinances below them — never grouped
    // by document type.
    let systematic_numbers: Vec<&str> =
        hits.iter().map(|h| h["sr"].as_str().expect("sr")).collect();
    assert_eq!(
        systematic_numbers,
        ["832.10", "832.12", "832.102", "832.104", "832.121"],
        "the collection's own order — the act before its ordinances, never by document type: {out}"
    );
    assert_eq!(hits[0]["eli"], KVG, "the KVG first: {out}");
    assert_eq!(hits[0]["in_force"], true);
    assert_eq!(
        hits[0]["sr"], "832.10",
        "sr from the taxonomy, in the same query"
    );
    assert!(hits[0]["title"]
        .as_str()
        .unwrap()
        .contains("Krankenversicherung"));
    assert_eq!(hits[0]["title_lang"], "de");
    assert_eq!(hits[0]["matched"], "title");
    assert_eq!(out["returned"], 5);
    assert_eq!(
        out["truncated"], true,
        "many ordinances match — the cap cut"
    );
    // Every hit says whether it is in force; the ones in force come
    // before the repealed ones.
    let first_repealed = hits.iter().position(|h| h["in_force"] == false);
    let last_in_force = hits.iter().rposition(|h| h["in_force"] == true);
    if let (Some(r), Some(f)) = (first_repealed, last_in_force) {
        assert!(f < r, "in force before repealed: {out}");
    }
    assert_eq!(
        out["abbreviation_tried"], false,
        "a long phrase is never an abbreviation"
    );

    // «datenschutz»: the nDSG (in force since 2023) before the 1992 act.
    let out = domain::search_law(&ctx, "datenschutz", Some(5)).expect("runs");
    assert_eq!(out["hits"][0]["eli"], NDSG, "{out}");
    assert_eq!(out["hits"][0]["in_force"], true);
}

/// BY point 0: an act found by its multi-word OFFICIAL TITLE.
///
/// The first live measurement asked for «Bundesgesetz über die
/// politischen Rechte» and was answered the UNO covenant; the BPR
/// turned up only on a second try, by abbreviation. The recorded
/// answer carries the reason in its own first hit: the graph writes
/// the title as «Bundesgesetz **vom 17. Dezember 1976** über die
/// politischen Rechte (BPR)», so the official title as a human writes
/// it is NOT a substring of it, and a single contiguous `CONTAINS`
/// could not match the act whose title it is. Every word must be found
/// in the same title instead.
#[test]
fn search_law_finds_an_act_by_its_multi_word_official_title() {
    let ctx = fixture_ctx();
    let out =
        domain::search_law(&ctx, "Bundesgesetz über die politischen Rechte", None).expect("runs");
    assert_eq!(
        out["abbreviation_tried"], false,
        "a five-word title is never an abbreviation"
    );
    let hits = out["hits"].as_array().expect("hits");
    assert_eq!(hits[0]["sr"], "161.1", "the BPR, first: {out}");
    assert_eq!(
        hits[0]["eli"], "https://fedlex.data.admin.ch/eli/cc/1978/688_688_688",
        "{out}"
    );
    assert_eq!(hits[0]["in_force"], true);
    assert_eq!(hits[0]["matched"], "title");

    // The defect, in the answer's own words: the query is not a
    // substring of the title it names.
    let title = hits[0]["title"].as_str().expect("a German title");
    assert!(
        title.contains("Bundesgesetz vom 17. Dezember 1976 über die politischen Rechte"),
        "the graph interpolates the promulgation date: {title}"
    );
    assert!(
        !title.contains("Bundesgesetz über die politischen Rechte"),
        "…which is exactly why the contiguous filter could not find it: {title}"
    );
    assert!(
        hits.iter().all(|h| h["sr"] != "0.103.2"),
        "and a covenant that shares two words but not the act type is not in the window: {out}"
    );
}

/// …and where the treaty IS in the window — «politische Rechte» is a
/// substring of both titles — the systematic order decides: 161.1 (four
/// digits) before 0.103.2 (five). The RANKING was never the defect,
/// and this is what says so.
#[test]
fn search_law_ranks_the_act_above_the_treaty_that_shares_its_words() {
    let ctx = fixture_ctx();
    let out = domain::search_law(&ctx, "politische Rechte", None).expect("runs");
    let hits = out["hits"].as_array().expect("hits");
    let bpr = hits
        .iter()
        .position(|h| h["sr"] == "161.1")
        .unwrap_or_else(|| panic!("the BPR is in the window: {out}"));
    assert_eq!(bpr, 0, "the act that governs the field stands first: {out}");
    if let Some(treaty) = hits.iter().position(|h| h["sr"] == "0.103.2") {
        assert!(bpr < treaty, "the act before the treaty: {out}");
    }
    // German inflection is handled by the word-wise substring itself:
    // «politische» is a substring of «politischen».
    assert!(
        hits[0]["title"]
            .as_str()
            .is_some_and(|t| t.contains("politischen Rechte")),
        "the inflected form matched: {out}"
    );
}

/// A query that is a pasted sentence rather than a title is refused
/// before a request: every word becomes one `CONTAINS`, and the
/// endpoint is shared.
#[test]
fn search_law_refuses_a_sentence_before_it_asks() {
    let ctx = fixture_ctx();
    let sentence = "Ich suche das Bundesgesetz über die politischen Rechte und zwar die Fassung \
                    die heute in Kraft steht bitte";
    let out = domain::search_law(&ctx, sentence, None).expect("runs");
    assert_eq!(out["error"], "invalid-input", "{out}");
    assert!(
        out["detail"]
            .as_str()
            .is_some_and(|d| d.contains("at most 12 words")),
        "the refusal names the cap and what to do instead: {out}"
    );
}

/// BO′: an official abbreviation resolves EXACTLY over
/// `jolux:titleShort` (verified live: «StPO» → cc/2010/267) and ranks
/// before every substring match.
#[test]
fn search_law_resolves_official_abbreviations_first() {
    let ctx = fixture_ctx();
    let out = domain::search_law(&ctx, "StPO", None).expect("runs");
    assert_eq!(out["abbreviation_tried"], true);
    let hits = out["hits"].as_array().expect("hits");
    assert_eq!(hits[0]["eli"], STRAFPROZESSORDNUNG, "{out}");
    assert_eq!(hits[0]["matched"], "abbreviation");
    assert_eq!(hits[0]["abbreviation"], "StPO");
    assert_eq!(hits[0]["in_force"], true);
    // «OR»: the abbreviation group comes first even though the
    // substring «or» matches half the collection (VerORdnung …).
    let out = domain::search_law(&ctx, "OR", None).expect("runs");
    let hits = out["hits"].as_array().expect("hits");
    assert_eq!(hits[0]["matched"], "abbreviation", "{out}");
    assert!(hits[0]["abbreviation"]
        .as_str()
        .is_some_and(|a| a.eq_ignore_ascii_case("OR")));
    assert!(
        hits[0]["title"]
            .as_str()
            .unwrap()
            .contains("Obligationenrecht"),
        "{out}"
    );
    assert_eq!(out["truncated"], true);
}

/// BO′: an empty result says what search_law cannot do.
#[test]
fn search_law_says_what_it_cannot_do_when_nothing_matches() {
    let out = domain::search_law(&fixture_ctx(), "Quantencomputergesetz", None).expect("runs");
    assert_eq!(out["kind"], "hint");
    assert_eq!(out["returned"], 0);
    assert_eq!(out["truncated"], false);
    let hint = out["hint"].as_str().expect("the hint sentence");
    assert!(hint.contains("not a full-text search"));
    assert!(hint.contains("search_text"));
}

#[test]
fn metadata_carries_profile_and_provenance() {
    let out = domain::get_law_metadata(&fixture_ctx(), KVG, Some("2020-01-01")).expect("runs");
    assert_eq!(out["kind"], "norm");
    assert_eq!(
        out["provenance"]["valid_as_of"], "2020-01-01",
        "as_of echoed resolved"
    );
    assert!(out["dates"].get("document").is_some() || out["dates"].get("entry_in_force").is_some());
    // J1.2: genre and responsible office live on the official-compilation
    // level — the consolidated profile never promises them.
    assert!(
        out.get("genre").is_none() && out.get("responsible_office").is_none(),
        "the profile promises neither genre nor office: {out}"
    );
}

#[test]
fn unknown_eli_is_not_found_and_foreign_iri_is_refused() {
    let ctx = fixture_ctx();
    let out = domain::get_law_metadata(
        &ctx,
        "https://fedlex.data.admin.ch/eli/cc/1900/000_000_000",
        None,
    )
    .expect("runs");
    assert_eq!(out["error"], "not-found");
    let out = domain::get_law_metadata(&ctx, "https://evil.example/eli/x", None).expect("runs");
    assert_eq!(
        out["error"], "invalid-input",
        "non-Fedlex IRIs are refused by construction"
    );
}

#[test]
fn the_bitemporal_loop_resolves_governing_versions_honestly() {
    let ctx = fixture_ctx();
    let versions = domain::list_versions(&ctx, KVG).expect("runs");
    let list = versions["versions"].as_array().expect("versions");
    assert!(
        list.len() > 10,
        "the KVG carries many consolidations incl. future ones"
    );
    // J14.1b: the list says how many it holds, in the collection's own
    // order — by applicability date, ascending from the first one.
    assert_eq!(
        versions["total"].as_u64().unwrap() as usize,
        list.len(),
        "{versions}"
    );
    let dates: Vec<&str> = list
        .iter()
        .map(|v| v["date"].as_str().expect("date"))
        .collect();
    assert!(
        dates.windows(2).all(|w| w[0] <= w[1]),
        "ordered by applicability date: {dates:?}"
    );
    assert_eq!(dates[0], "1996-01-01", "{versions}");
    assert!(
        list.len() <= 200,
        "capped by the query's LIMIT 200: {}",
        list.len()
    );
    // X19.9: a consolidation dated far beyond today is decided data, not
    // a fault — the list keeps it instead of filtering it away.
    assert!(
        list.iter()
            .any(|v| v["eli_version"] == format!("{KVG}/20320701") && v["date"] == "2032-07-01"),
        "a consolidation dated far after today is data, not a fault — the list keeps it: {versions}"
    );

    // Governing version at a mid-2026 date: the 2026-07-01 consolidation.
    let governing = domain::resolve_consolidation_at(&ctx, KVG, "2026-08-20").expect("runs");
    assert_eq!(
        governing["eli_version"],
        format!("{KVG}/20260701"),
        "max dateApplicability <= as_of"
    );
    assert_eq!(governing["valid_as_of"], "2026-07-01");

    // Before first entry into force: honest not-found …
    let early = domain::resolve_consolidation_at(&ctx, KVG, "1900-01-01").expect("runs");
    assert_eq!(early["error"], "not-found");

    // … and check_in_force turns exactly that into a VALID false —
    // since BV read from the ACT's own entry date (1996-01-01), not
    // from the absence of a consolidation.
    let in_force = domain::check_in_force(&ctx, KVG, "1900-01-01").expect("runs");
    assert_eq!(
        in_force["in_force"], false,
        "false is an answer, not an error"
    );
    assert_eq!(
        in_force["dates"]["entry_in_force"], "1996-01-01",
        "{in_force}"
    );
    assert!(in_force["governing_version"].is_null(), "{in_force}");
    let in_force_now = domain::check_in_force(&ctx, KVG, "2026-08-20").expect("runs");
    assert_eq!(in_force_now["in_force"], true);
    assert_eq!(in_force_now["governing_version"], format!("{KVG}/20260701"));
    assert_eq!(in_force_now["future_as_of"], false);
    // BO′: a Stichtag after today is a PROJECTION from the decided
    // consolidations the graph already carries — marked, never a
    // finding (fixture today = 2026-08-21).
    let projected = domain::check_in_force(&ctx, KVG, "2030-01-01").expect("runs");
    assert_eq!(projected["in_force"], true);
    assert_eq!(projected["governing_version"], format!("{KVG}/20280101"));
    assert_eq!(projected["future_as_of"], true, "{projected}");
    assert_eq!(in_force["future_as_of"], false, "1900 is not the future");
}

/// BV addendum 1 (J3.1/J3.2): whether an act is in force is read from
/// the ACT's own dates — entry into force and the earlier of the two
/// end dates — not from whether a consolidation happens to govern the
/// day. The recorded case is the Energy Act of 1998, repealed on
/// 2018-01-01: in force in 2017, not in force today, and the answer
/// names the date that ended it.
#[test]
fn check_in_force_reads_the_acts_own_end_of_force() {
    let ctx = fixture_ctx();
    let while_in_force = domain::check_in_force(&ctx, ENG_REPEALED, "2017-06-01").expect("runs");
    assert_eq!(while_in_force["in_force"], true, "{while_in_force}");
    assert_eq!(while_in_force["dates"]["entry_in_force"], "1999-01-01");
    assert_eq!(while_in_force["dates"]["no_longer_in_force"], "2018-01-01");
    // J3.2: an act may carry two end dates and they disagree on about
    // 4 % of expired acts — the answer names the field that decided,
    // so a reader never has to guess between the dates beside it.
    assert_eq!(
        while_in_force["decided_by"], "entry_in_force",
        "in force because it had started and no end date had passed: {while_in_force}"
    );
    assert_eq!(while_in_force["status_label"], "Nicht mehr in Kraft");
    assert_eq!(while_in_force["kind"], "norm");

    let today = domain::check_in_force(&ctx, ENG_REPEALED, "2026-08-29").expect("runs");
    assert_eq!(
        today["decided_by"], "no_longer_in_force",
        "and out of force BY that date, named: {today}"
    );
    assert_eq!(
        today["in_force"], false,
        "the act ended on 2018-01-01: {today}"
    );
    assert_eq!(today["dates"]["no_longer_in_force"], "2018-01-01");
    assert_eq!(today["status_unset"], false);
    assert_eq!(today["no_enforcement_data"], false);
    assert!(
        today["note"]
            .as_str()
            .is_some_and(|n| n.contains("EARLIER of the two")),
        "{today}"
    );
    // The day the act ended is NOT a day it was in force (the end date
    // is exclusive by the rule: an end date <= as_of ends it).
    let last_day = domain::check_in_force(&ctx, ENG_REPEALED, "2018-01-01").expect("runs");
    assert_eq!(last_day["in_force"], false, "{last_day}");
    let day_before = domain::check_in_force(&ctx, ENG_REPEALED, "2017-12-31").expect("runs");
    assert_eq!(day_before["in_force"], true, "{day_before}");
    // And the act in force answers true with its own entry date.
    let kvg = domain::check_in_force(&ctx, KVG, "2026-08-20").expect("runs");
    assert_eq!(kvg["in_force"], true);
    assert_eq!(kvg["dates"]["entry_in_force"], "1996-01-01");
    // J3.1: the two dates come from different levels — entry_in_force is
    // the ACT's own date, while dateApplicability belongs to the
    // consolidation that governs the day asked for.
    assert_eq!(
        kvg["governing_version"],
        format!("{KVG}/20260701"),
        "entry_in_force is the ACT's own date; dateApplicability is the CONSOLIDATION's: {kvg}"
    );
    assert!(kvg["dates"]["no_longer_in_force"].is_null());
    assert_eq!(kvg["status_label"], "In Kraft");
}

/// BV addendum 2 (J3.3): an act the graph knows but never consolidated
/// is an ANSWER, not a not-found — `list_versions` answers an empty
/// list with its reason, `check_in_force` answers from the profile and
/// says that the graph carries neither status nor date. Only an ELI the
/// graph knows nothing about is a not-found.
#[test]
fn an_act_without_consolidations_answers_from_its_profile() {
    let ctx = fixture_ctx();
    let versions = domain::list_versions(&ctx, DSG_STUB).expect("runs");
    assert!(versions.get("error").is_none(), "{versions}");
    assert_eq!(versions["total"], 0);
    assert_eq!(versions["versions"].as_array().unwrap().len(), 0);
    assert!(
        versions["note"]
            .as_str()
            .is_some_and(|n| n.contains("carries no consolidation")),
        "{versions}"
    );
    let unknown = domain::list_versions(&ctx, UNKNOWN).expect("runs");
    assert_eq!(unknown["error"], "not-found", "{unknown}");

    let force = domain::check_in_force(&ctx, DSG_STUB, "2026-08-29").expect("runs");
    assert!(force.get("error").is_none(), "{force}");
    assert_eq!(force["in_force"], false);
    assert_eq!(force["status_unset"], true, "{force}");
    assert_eq!(force["no_enforcement_data"], true);
    assert!(force["governing_version"].is_null());
    assert!(
        force["note"]
            .as_str()
            .is_some_and(|n| n.contains("not «out of force»")),
        "{force}"
    );
    let unknown_force = domain::check_in_force(&ctx, UNKNOWN, "2026-08-29").expect("runs");
    assert_eq!(unknown_force["error"], "not-found", "{unknown_force}");
}

/// BV addendum 3 (X18.7): text that sits directly under `<body>` — the
/// ECHR's closing signature block — must not vanish. It is rendered by
/// `read_document`, named by `get_structure` and found by
/// `search_text`; before BV all three lost it.
#[test]
fn body_level_text_is_rendered_named_and_searchable() {
    let ctx = fixture_ctx();
    let version = "https://fedlex.data.admin.ch/eli/cc/1974/2151_2151_2151/20220916";
    let document = domain::read_document(&ctx, version, Some("de"), None, None).expect("runs");
    let markdown = document["markdown"].as_str().expect("markdown");
    assert!(
        markdown.contains("Urschrift"),
        "the signature block is part of the act: {}",
        &markdown[markdown.len().saturating_sub(200)..]
    );
    let named = document["body_level_elements"].as_array().expect("named");
    assert_eq!(named.len(), 1, "{named:?}");
    assert_eq!(named[0]["tag"], "signature");
    assert_eq!(named[0]["position"], "after the hierarchy");
    assert!(named[0]["chars"].as_u64().unwrap() > 100);
    assert!(
        document["note"]
            .as_str()
            .is_some_and(|n| n.contains("signature")),
        "{document}"
    );

    let outline = domain::get_structure(&ctx, version, Some("de"), None).expect("runs");
    assert_eq!(outline["body_level_elements"][0]["tag"], "signature");
    assert!(
        outline["note"]
            .as_str()
            .is_some_and(|n| n.contains("read_article cannot open them")),
        "{outline}"
    );

    let hits = domain::search_text(&ctx, version, "Urschrift", Some("de"), None).expect("runs");
    assert_eq!(hits["body_level_hits"], 1, "{hits}");
    let hit = &hits["hits"].as_array().unwrap()[0];
    assert!(hit["eid"].is_null(), "{hit}");
    assert_eq!(hit["element_kind"], "signature");
    assert!(hit["snippet"].as_str().unwrap().contains("Urschrift"));
    // An act whose body carries nothing but hierarchy says so with an
    // empty list and no note.
    let bgoe = domain::get_structure(&ctx, BGOE_VERSION, Some("de"), None).expect("runs");
    assert_eq!(bgoe["body_level_elements"].as_array().unwrap().len(), 0);
    assert!(bgoe["note"].is_null(), "{bgoe}");
}

/// BV addendum 5 (J16.1): the impact directions say what they are. The
/// field is the FORESEEN impact — 0.8 % of the impact graph, no type,
/// no date — and what the recorded KVG answer carries at the incoming
/// end is 33 consultation drafts, not amending acts.
#[test]
fn the_impact_directions_say_what_they_are() {
    let ctx = fixture_ctx();
    let incoming = domain::get_citations(&ctx, KVG, "in").expect("runs");
    let coverage = incoming["coverage"].as_str().expect("coverage");
    assert!(
        coverage.contains("foreseenImpactToLegalResource"),
        "{coverage}"
    );
    assert!(coverage.contains("0.8 %"), "{coverage}");
    assert!(coverage.contains("consultation drafts"), "{coverage}");
    assert!(
        coverage.contains("fedlex.get_article_history"),
        "«who amended this act» must be pointed at the tool that answers it: {coverage}"
    );
    let rows = incoming["citations"].as_array().expect("citations");
    assert_eq!(rows.len(), 33, "{incoming}");
    assert!(
        rows.iter().all(|c| c["from"]
            .as_str()
            .is_some_and(|f| f.contains("/eli/dl/proj/"))),
        "every incoming foreseen impact of the KVG is a consultation draft: {:?}",
        rows.first()
    );
    // The stage-one line no longer promises «who amends X».
    let line = FedlexServer::tool_router()
        .list_all()
        .into_iter()
        .find(|t| t.name == "fedlex.get_citations")
        .and_then(|t| t.description.map(|d| d.to_string()))
        .expect("the tool is mounted");
    assert!(!line.contains("who amends"), "{line}");
    assert!(line.contains("cites"), "{line}");
    // J16.1: the profile exposes a second ignorable field — the
    // Dublin-Core identifier — and the answer carries it as a bare
    // number. Since A′ the note says how thin it is, which is what
    // «where it exposes one it says how thin it is» has to mean.
    let profile = domain::get_law_metadata(&ctx, KVG, None).expect("runs");
    assert_eq!(
        profile["identifier"], "19940073",
        "the Dublin-Core identifier IS exposed: {profile}"
    );
    assert!(
        profile["note"]
            .as_str()
            .is_some_and(|n| n.contains("Dublin-Core number") && n.contains("never to be built on")),
        "…and the answer says how thin it is: {profile}"
    );
}

/// BV addendum 6 (J5.3): the opaque status IRI is decoded wherever an
/// act is answered — the profile, the SR resolution and its
/// also_matches rows — and every one of them derives `in_force` from
/// the same rule as `check_in_force`.
#[test]
fn the_status_vocabulary_is_decoded_in_every_act_answer() {
    let ctx = fixture_ctx();
    let profile = domain::get_law_metadata(&ctx, KVG, None).expect("runs");
    assert_eq!(profile["status_label"], "In Kraft", "{profile}");
    assert_eq!(profile["status_unset"], false);
    assert_eq!(profile["in_force"], true);
    let repealed = domain::get_law_metadata(&ctx, ENG_REPEALED, None).expect("runs");
    assert_eq!(repealed["status_label"], "Nicht mehr in Kraft");
    assert_eq!(repealed["in_force"], false, "{repealed}");
    assert_eq!(repealed["dates"]["no_longer_in_force"], "2018-01-01");
    let stub = domain::get_law_metadata(&ctx, DSG_STUB, None).expect("runs");
    assert!(stub["status_label"].is_null());
    assert_eq!(stub["status_unset"], true, "{stub}");

    let sr = domain::resolve_sr(&ctx, "832.10").expect("runs");
    assert_eq!(sr["status_label"], "In Kraft", "{sr}");
    let also = sr["also_matches"].as_array().expect("also_matches");
    assert!(!also.is_empty(), "{sr}");
    for row in also {
        assert!(row["in_force"].is_boolean(), "{row}");
        assert!(
            row["status_label"].is_string() || row["status_unset"] == true,
            "a decoded label or an honest «no status»: {row}"
        );
    }
    // parse_reference's SR branch rides the same answer.
    let parsed = domain::parse_reference(&ctx, "SR 832.10").expect("runs");
    assert_eq!(parsed["references"][0]["act"]["sr"], "832.10", "{parsed}");
    assert_eq!(parsed["references"][0]["act"]["in_force"], true);
}

/// BV addendum 7 (X15.3): the citation pair reports the eId resolution
/// exactly as `read_article` does — how many other elements carry the
/// same address, and whether normalisation was needed to find it.
#[test]
fn the_citation_pair_reports_the_eid_resolution() {
    let ctx = fixture_ctx();
    let kvg_version = recorded_version_for(KVG).expect("a recorded KVG manifestation");
    let quoted = domain::check_quote(
        &ctx,
        &kvg_version,
        "art_25_a",
        "Pflegeleistungen bei Krankheit",
        Some("de"),
    )
    .expect("runs");
    assert_eq!(quoted["verified"], true, "{quoted}");
    assert_eq!(quoted["eid_duplicates"], 0);
    assert_eq!(quoted["eid_via_normalisation"], false);
    assert!(
        quoted["note"]
            .as_str()
            .is_some_and(|n| n.contains("eid_duplicates")),
        "{quoted}"
    );
    let cited = domain::cite(&ctx, &kvg_version, "art_25_a", Some("de")).expect("runs");
    assert_eq!(cited["eid_duplicates"], 0);
    assert_eq!(cited["eid_via_normalisation"], false);
    assert!(cited["note"]
        .as_str()
        .is_some_and(|n| n.contains("eid_duplicates")));
    // The annex wrapper resolves to its first level and reports THAT
    // element's resolution.
    let annex = domain::cite(&ctx, LSV_VERSION, "annex_3", Some("de")).expect("runs");
    assert_eq!(annex["eid"], "annex_3/lvl_u1");
    assert_eq!(annex["eid_duplicates"], 0, "{annex}");
}

#[test]
fn citations_are_direction_typed_with_honest_v0_coverage() {
    let ctx = fixture_ctx();
    let incoming = domain::get_citations(&ctx, KVG, "in").expect("runs");
    assert_eq!(incoming["direction"], "in");
    assert!(incoming["coverage"]
        .as_str()
        .unwrap()
        .contains("impact graph"));
    let bad = domain::get_citations(&ctx, KVG, "sideways").expect("runs");
    assert_eq!(bad["error"], "invalid-input");
}

#[test]
fn read_article_delivers_eid_precise_norm_text_from_the_recorded_manifestation() {
    let ctx = fixture_ctx();
    let out = domain::read_article(&ctx, BGOE_VERSION, "art_6", Some("de")).expect("runs");
    assert_eq!(out["kind"], "norm", "{out}");
    assert_eq!(out["eid"], "art_6");
    assert_eq!(out["element_kind"], "article");
    assert_eq!(out["heading"], "Öffentlichkeitsprinzip");
    assert!(out["text"]
        .as_str()
        .unwrap()
        .contains("amtliche Dokumente einzusehen"));
    assert_eq!(out["provenance"]["valid_as_of"], "2023-11-01");
    // J0.1: an answer read off the XML names the side that spoke —
    // `served` is the field only the manifestation family carries.
    assert_eq!(
        out["provenance"]["served"], "fixture",
        "an XML answer names the side that spoke: {out}"
    );
    // The manifestation host is the declared egress, enforced.
    assert!(out["manifestation_url"]
        .as_str()
        .unwrap()
        .starts_with(oh_mcp_fedlex::backend::MANIFESTATION_HOST));
}

#[test]
fn read_article_accepts_path_eids_and_refuses_malformed_ones() {
    let ctx = fixture_ctx();
    // A paragraph by its path eId (what search_text hands out) …
    let para = domain::read_article(&ctx, BGOE_VERSION, "art_2/para_2", Some("de")).expect("runs");
    assert_eq!(para["element_kind"], "paragraph", "{para}");
    assert!(para["text"].as_str().unwrap().contains("Nationalbank"));
    // … an annex level by the path eId list_annexes names …
    let annex =
        domain::read_article(&ctx, BGOE_VERSION, "annex_u1/lvl_u1", Some("de")).expect("runs");
    assert_eq!(annex["element_kind"], "level", "{annex}");
    assert!(annex["heading"]
        .as_str()
        .unwrap()
        .contains("Änderung bisherigen Rechts"));
    // … and the gate is BEFORE any fetch.
    for bad in ["", "art_1/", "/art_1", "art_1/../x", "art 1", "<art>"] {
        let out = domain::read_article(&ctx, BGOE_VERSION, bad, Some("de")).expect("runs");
        assert_eq!(out["error"], "invalid-input", "eid «{bad}»: {out}");
    }
    // An unknown eId names a way forward.
    let missing = domain::read_article(&ctx, BGOE_VERSION, "art_999", Some("de")).expect("runs");
    assert_eq!(missing["error"], "not-found");
    assert!(missing["detail"]
        .as_str()
        .unwrap()
        .contains("get_structure"));
}

// --- BQ wave 1, A: the XML tools on the BGÖ fixture -------------------

/// BV, rules J18.2/X9.4 and X15.3: the JOLux spelling of an eId
/// (`art_25a`) opens the manifestation's element (`art_25_a`), and the
/// answer says it took normalisation to get there; an unambiguous
/// lookup reports no duplicates.
#[test]
fn read_article_accepts_the_jolux_spelling_and_says_how_it_resolved() {
    let ctx = fixture_ctx();
    let kvg_version = recorded_version_for(KVG).expect("a recorded KVG manifestation");
    let normalised = domain::read_article(&ctx, &kvg_version, "art_25a", Some("de")).expect("runs");
    assert_eq!(
        normalised["eid"], "art_25_a",
        "the document's own spelling: {normalised}"
    );
    assert_eq!(normalised["eid_via_normalisation"], true, "{normalised}");
    assert_eq!(normalised["eid_duplicates"], 0);
    let exact = domain::read_article(&ctx, &kvg_version, "art_25_a", Some("de")).expect("runs");
    assert_eq!(exact["eid_via_normalisation"], false, "{exact}");
    assert_eq!(exact["eid_duplicates"], 0);
    assert_eq!(
        exact["text"], normalised["text"],
        "the same element, either way"
    );
}

/// BV, rule J13.2: eIds are language-invariant — the same address
/// reads in de, fr and it, and the texts differ while the eId does not.
#[test]
fn the_same_eid_reads_in_every_language_of_the_version() {
    let ctx = fixture_ctx();
    let mut texts = Vec::new();
    for lang in ["de", "fr", "it"] {
        let out = domain::read_article(&ctx, BGOE_VERSION, "art_6", Some(lang)).expect("runs");
        assert!(out.get("error").is_none(), "{lang}: {out}");
        assert_eq!(
            out["eid"], "art_6",
            "the eId does not move with the language: {out}"
        );
        texts.push(out["text"].as_str().expect("text").to_string());
    }
    assert!(
        texts[0].starts_with("Art. 6 Öffentlichkeitsprinzip"),
        "{}",
        texts[0]
    );
    assert!(
        texts[1].contains("Principe de la transparence"),
        "{}",
        texts[1]
    );
    assert!(
        texts[2].contains("Principio della trasparenza"),
        "{}",
        texts[2]
    );
    assert_ne!(texts[0], texts[1]);
}

/// BV, rule J3.3: 15 % of the acts carry no `inForceStatus` at all —
/// an OPTIONAL clause keeps them, and the answer says `in_force:
/// false` with a null status rather than dropping the act.
#[test]
fn search_law_keeps_the_acts_that_carry_no_in_force_status() {
    let ctx = fixture_ctx();
    let out = domain::search_law(&ctx, "StPO", Some(10)).expect("runs");
    let hits = out["hits"].as_array().expect("hits");
    let statusless: Vec<&serde_json::Value> =
        hits.iter().filter(|h| h["status"].is_null()).collect();
    assert!(
        !statusless.is_empty(),
        "the recorded window carries acts without a status — they must survive the OPTIONAL: {out}"
    );
    for hit in &statusless {
        assert_eq!(hit["in_force"], false, "no status is not «in force»: {hit}");
        assert!(
            hit["eli"].as_str().is_some_and(|e| e.contains("/eli/")),
            "{hit}"
        );
        // J17.2: the same holds for the second ignorable field — no
        // taxonomy entry either, and the act is kept all the same.
        assert!(
            hit["sr"].is_null(),
            "no taxonomy entry either — the second OPTIONAL keeps the act instead of dropping it: {hit}"
        );
        assert!(
            hit["title"].as_str().is_some_and(|t| !t.is_empty()),
            "and it still carries its title: {hit}"
        );
    }
    // And the ranking puts an act in force first all the same.
    assert_eq!(hits[0]["in_force"], true, "{out}");
}

/// BV, rule X19.6: every linked reference of the authentic text points
/// at a WORK — never at a language expression or a format.
#[test]
fn linked_references_point_at_works_never_at_expressions() {
    let ctx = fixture_ctx();
    let out = domain::get_references(&ctx, BGOE_VERSION, None, None, None, None).expect("runs");
    let refs = out["references"].as_array().expect("references");
    assert!(refs.len() > 20, "{out}");
    let linked: Vec<&str> = refs
        .iter()
        .filter_map(|r| r["href"].as_str())
        .filter(|href| href.contains("fedlex.data.admin.ch/eli/"))
        .collect();
    assert!(!linked.is_empty(), "{out}");
    for href in &linked {
        let tail = href.split("/eli/").nth(1).expect("an eli path");
        assert_eq!(
            tail.matches('/').count(),
            2,
            "a work is <collection>/<year>/<nr> — «{href}» carries a language or format segment"
        );
        assert!(!href.ends_with("/de") && !href.ends_with("/xml"), "{href}");
    }
    assert!(
        out["coverage"]
            .as_str()
            .is_some_and(|c| c.contains("work level")),
        "{out}"
    );
}

#[test]
fn get_structure_outlines_the_act_with_eids_and_headings() {
    let ctx = fixture_ctx();
    let out = domain::get_structure(&ctx, BGOE_VERSION, None, None).expect("runs");
    assert_eq!(out["kind"], "norm", "{out}");
    assert_eq!(out["depth"], "article");
    let sections = out["structure"].as_array().expect("sections");
    assert_eq!(sections.len(), 5, "the BGÖ has five sections");
    assert_eq!(sections[0]["eid"], "sec_1");
    assert_eq!(sections[0]["kind"], "section");
    assert_eq!(sections[0]["heading"], "Allgemeine Bestimmungen");
    let articles = sections[0]["children"].as_array().expect("articles");
    assert_eq!(articles[0]["eid"], "art_1");
    assert_eq!(articles[0]["num"], "Art. 1");
    assert_eq!(articles[0]["heading"], "Zweck und Gegenstand");
    assert!(
        articles[0].get("children").is_none(),
        "depth=article cuts below the article"
    );
    assert_eq!(out["truncated"], false);
    assert_eq!(out["nodes_total"], out["nodes_returned"]);
    assert_eq!(out["annexes"], 1);
    assert_eq!(out["provenance"]["valid_as_of"], "2023-11-01");

    // depth=full keeps the paragraphs; the node count grows.
    let full = domain::get_structure(&ctx, BGOE_VERSION, None, Some("full")).expect("runs");
    assert!(full["nodes_total"].as_u64() > out["nodes_total"].as_u64());
    assert!(full["structure"][0]["children"][1]["children"]
        .as_array()
        .is_some_and(|paras| paras.iter().any(|p| p["eid"] == "art_2/para_1")));
    // J4.3: a paragraph address is read from the manifestation — the
    // answer says so with `served`, and the graph's own catalogue of
    // the same act carries no address below the article at all.
    assert_eq!(
        full["provenance"]["served"], "fixture",
        "the sub-article outline is read from the manifestation, not the graph: {full}"
    );
    let graph = domain::get_subdivisions(&ctx, BGOE).expect("runs");
    assert!(
        graph["subdivisions"]
            .as_array()
            .unwrap()
            .iter()
            .all(|s| !s["eid"].as_str().unwrap_or("").contains("para")),
        "a paragraph address cannot be validated against the graph: {graph}"
    );

    let bad = domain::get_structure(&ctx, BGOE_VERSION, None, Some("deep")).expect("runs");
    assert_eq!(bad["error"], "invalid-input");
}

#[test]
fn search_text_is_a_hint_with_total_and_truncation() {
    let ctx = fixture_ctx();
    let out = domain::search_text(&ctx, BGOE_VERSION, "Zugang", None, Some(3)).expect("runs");
    assert_eq!(out["kind"], "hint", "{out}");
    let hits = out["hits"].as_array().expect("hits");
    assert_eq!(hits.len(), 3, "capped at limit");
    assert!(
        out["total"].as_u64().unwrap() > 3,
        "total counts beyond the cap"
    );
    assert_eq!(out["truncated"], true);
    for hit in hits {
        assert!(hit["snippet"]
            .as_str()
            .unwrap()
            .to_lowercase()
            .contains("zugang"));
        assert!(hit["eid"].as_str().is_some());
    }
    // Case-insensitive substring.
    let lower = domain::search_text(&ctx, BGOE_VERSION, "zUGANG", None, None).expect("runs");
    assert_eq!(lower["total"], out["total"]);
    // A miss is an empty list, not an error.
    let miss =
        domain::search_text(&ctx, BGOE_VERSION, "Quantencomputer", None, None).expect("runs");
    assert_eq!(miss["total"], 0);
    assert_eq!(miss["truncated"], false);
    let empty = domain::search_text(&ctx, BGOE_VERSION, "  ", None, None).expect("runs");
    assert_eq!(empty["error"], "invalid-input");
}

/// THE proof the package exists for: a model that knows the act but
/// not the article number reaches the norm text through search_text
/// → read_article, never by guessing.
#[test]
fn a_model_reaches_the_text_without_knowing_the_article_number() {
    let ctx = fixture_ctx();
    // Stage 1: «where does the BGÖ speak about Zugang?»
    let hits = domain::search_text(&ctx, BGOE_VERSION, "Zugang", None, Some(100)).expect("runs");
    assert_eq!(hits["kind"], "hint");
    let all_hits = hits["hits"].as_array().expect("hits");
    assert!(
        all_hits.iter().all(|h| h["article_eid"].as_str().is_some()),
        "every hit names the article it sits in: {all_hits:?}"
    );
    // The model reads HEADINGS, not numbers: it picks the hit whose
    // heading speaks of «Kostenloser Zugang» and only then learns the
    // address. The number is an output of the tools, never an input.
    let first = all_hits
        .iter()
        .find(|h| {
            h["heading"]
                .as_str()
                .is_some_and(|heading| heading.contains("Kostenloser Zugang"))
        })
        .expect("a hit under the heading «Kostenloser Zugang …»")
        .clone();
    let eid = first["eid"].as_str().expect("hit eid").to_string();
    let article_eid = first["article_eid"]
        .as_str()
        .expect("the hit names its article")
        .to_string();
    assert_eq!(article_eid, "art_17", "learned from the heading, not typed");
    assert!(eid.starts_with(&format!("{article_eid}/")), "{eid}");
    // Stage 2: read exactly that element — by the hit's eId …
    let element = domain::read_article(&ctx, BGOE_VERSION, &eid, None).expect("runs");
    assert_eq!(element["kind"], "norm", "{element}");
    assert!(element["text"]
        .as_str()
        .unwrap()
        .to_lowercase()
        .contains("zugang"));
    // … or the whole article the hit names.
    let article = domain::read_article(&ctx, BGOE_VERSION, &article_eid, None).expect("runs");
    assert_eq!(article["kind"], "norm");
    assert_eq!(
        article["heading"],
        "Kostenloser Zugang zu amtlichen Dokumenten"
    );
    assert!(article["text"]
        .as_str()
        .unwrap()
        .to_lowercase()
        .contains("zugang"));
    // The other way in: the outline names the article without a guess.
    let outline = domain::get_structure(&ctx, BGOE_VERSION, None, None).expect("runs");
    let named = outline["structure"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|s| s["children"].as_array().cloned().unwrap_or_default())
        .find(|a| a["heading"] == "Kostenloser Zugang zu amtlichen Dokumenten")
        .expect("the outline carries the heading");
    assert_eq!(named["eid"], article_eid, "both ways name the same element");
}

#[test]
fn read_document_is_capped_with_the_original_length_and_a_continuation() {
    let ctx = fixture_ctx();
    let whole = domain::read_document(&ctx, BGOE_VERSION, None, None, None).expect("runs");
    assert_eq!(whole["kind"], "norm", "{whole}");
    assert_eq!(whole["truncated"], false);
    let total = whole["total_chars"].as_u64().unwrap();
    assert!(
        total > 5_000,
        "the BGÖ renders to a few thousand characters: {total}"
    );
    let md = whole["markdown"].as_str().unwrap();
    assert!(
        md.starts_with("# Bundesgesetz"),
        "title first: {}",
        &md[..80]
    );
    assert!(md.contains("Art. 6"));
    // X10.2: the default cap on a whole document sits far above the
    // article median of some 550 characters, and a per-element answer
    // carries no cap at all.
    assert_eq!(
        whole["max_chars"], 120_000,
        "the default document cap is far above the ~550-character article median: {whole}"
    );
    let element = domain::read_article(&ctx, BGOE_VERSION, "art_6", Some("de")).expect("runs");
    assert!(
        element.get("max_chars").is_none() && element.get("truncated").is_none(),
        "a per-element answer carries no cap at all: {element}"
    );

    let first = domain::read_document(&ctx, BGOE_VERSION, None, Some(1000), None).expect("runs");
    assert_eq!(first["truncated"], true);
    assert_eq!(first["total_chars"], total);
    assert_eq!(first["markdown"].as_str().unwrap().chars().count(), 1000);
    let next = first["next_offset"].as_u64().expect("continuation") as u32;
    assert_eq!(next, 1000);
    let second =
        domain::read_document(&ctx, BGOE_VERSION, None, Some(1000), Some(next)).expect("runs");
    assert_eq!(second["offset"], 1000);
    // J13.3: the cap and the total are measured on the text actually
    // served — the French manifestation, never assumed from the German.
    let french =
        domain::read_document(&ctx, BGOE_VERSION, Some("fr"), Some(1000), None).expect("runs");
    assert_eq!(french["lang"], "fr", "{french}");
    assert_eq!(french["truncated"], true);
    assert_eq!(french["markdown"].as_str().unwrap().chars().count(), 1000);
    assert_ne!(
        french["total_chars"], whole["total_chars"],
        "the cap and the total are measured on the French text, never assumed from the German: {french}"
    );
    let beyond =
        domain::read_document(&ctx, BGOE_VERSION, None, None, Some(10_000_000)).expect("runs");
    assert_eq!(beyond["error"], "invalid-input");
}

#[test]
fn get_references_lists_linked_and_unlinked_refs_as_hints_with_scope() {
    let ctx = fixture_ctx();
    let all = domain::get_references(&ctx, BGOE_VERSION, None, None, None, None).expect("runs");
    assert_eq!(all["kind"], "hint", "{all}");
    let total = all["total"].as_u64().unwrap();
    assert!(total > 30, "the BGÖ carries dozens of refs: {total}");
    assert_eq!(all["truncated"], false);
    // X11.2: 15 % of the corpus's references carry no target. They are
    // kept and COUNTED — the honest value for this manifestation is 0,
    // because no recorded manifestation carries a single one.
    assert_eq!(all["unlinked"], 0, "{all}");
    // J7.3: and the answer points at the other half of the picture.
    let coverage = all["coverage"].as_str().expect("coverage");
    assert!(
        coverage.contains("fedlex.get_citations") && coverage.contains("0 to 48 %"),
        "in-text references and formal citations each name the other: {coverage}"
    );
    let refs = all["references"].as_array().unwrap();
    assert!(
        refs.iter()
            .any(|r| r["href"] == "https://fedlex.data.admin.ch/eli/cc/1999/404"),
        "the preamble's BV reference is linked"
    );
    // Scoped to one article: only refs from within it.
    let scoped =
        domain::get_references(&ctx, BGOE_VERSION, Some("art_2"), None, None, None).expect("runs");
    let scoped_total = scoped["total"].as_u64().unwrap();
    assert!(scoped_total > 0 && scoped_total < total, "{scoped}");
    for r in scoped["references"].as_array().unwrap() {
        assert!(r["source_eid"].as_str().unwrap().starts_with("art_2"));
    }
    // Paging is honest.
    let page =
        domain::get_references(&ctx, BGOE_VERSION, None, None, Some(5), Some(2)).expect("runs");
    assert_eq!(page["references"].as_array().unwrap().len(), 5);
    assert_eq!(page["truncated"], true);
    assert_eq!(page["next_offset"], 7);
    let unknown = domain::get_references(&ctx, BGOE_VERSION, Some("art_999"), None, None, None)
        .expect("runs");
    assert_eq!(unknown["error"], "not-found");
    // The JOLux spelling of an eId (art_23a) scopes to the XML's
    // art_23_a — the same refs either way, never a silent zero.
    let jolux_form = domain::get_references(&ctx, BGOE_VERSION, Some("art_23a"), None, None, None)
        .expect("runs");
    let xml_form = domain::get_references(&ctx, BGOE_VERSION, Some("art_23_a"), None, None, None)
        .expect("runs");
    assert_eq!(jolux_form["eid"], "art_23_a", "{jolux_form}");
    assert!(jolux_form["total"].as_u64().unwrap() > 0);
    assert_eq!(jolux_form["total"], xml_form["total"]);
}

#[test]
fn get_modifications_anchors_change_notes_at_their_elements() {
    let ctx = fixture_ctx();
    let all = domain::get_modifications(&ctx, BGOE_VERSION, None, None).expect("runs");
    assert_eq!(all["kind"], "norm", "{all}");
    let notes = all["change_notes"].as_array().unwrap();
    assert!(notes.len() > 10, "the BGÖ carries many amendment footnotes");
    assert_eq!(all["change_notes_total"], notes.len());
    assert_eq!(
        all["mod_blocks_total"], 0,
        "a consolidation carries no <mod> blocks — they are worked in"
    );
    assert_eq!(all["mod_blocks_truncated"], false);
    // X7.2: a zero is a structural fact here, and the answer says why —
    // a caller must be able to tell it from a failed extraction.
    assert!(
        all["coverage"]
            .as_str()
            .unwrap()
            .contains("mod blocks exist on amending acts only"),
        "the tool says why the count is zero: {all}"
    );
    // J18.1: the answer names the side it came from — `served` on the
    // XML side, `source` on the graph side of the same interface.
    assert_eq!(
        all["provenance"]["served"], "fixture",
        "the XML side names where it came from: {all}"
    );
    let history = domain::get_article_history(&ctx, BGOE, "art_17").expect("runs");
    assert_eq!(
        history["provenance"]["source"], "fedlex.data.admin.ch/sparqlendpoint (live/base tier)",
        "{history}"
    );
    // Art. 2 Abs. 2 was amended by the FINMAG (in force 2009) — the
    // note is anchored at the paragraph and carries the AS ref.
    let scoped = domain::get_modifications(&ctx, BGOE_VERSION, Some("art_2"), None).expect("runs");
    let art2 = scoped["change_notes"].as_array().unwrap();
    assert!(!art2.is_empty());
    let finmag = art2
        .iter()
        .find(|n| n["anchor_eid"] == "art_2/para_2")
        .expect("the FINMAG note on Art. 2 Abs. 2");
    assert!(finmag["text"]
        .as_str()
        .unwrap()
        .contains("Finanzmarktaufsichtsgesetzes"));
    assert!(finmag["refs"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["href"].as_str().is_some_and(|h| h.contains("/eli/oc/"))));
    // X6.4: the notes are offered as their own answer and stay OUT of
    // the norm text — reading the same paragraph counts the note and
    // renders none of its wording.
    let para = domain::read_article(&ctx, BGOE_VERSION, "art_2/para_2", Some("de")).expect("runs");
    assert!(
        para["notes"].as_u64().unwrap() >= 1,
        "the note is counted: {para}"
    );
    assert!(
        !para["text"]
            .as_str()
            .unwrap()
            .contains("Finanzmarktaufsichtsgesetzes"),
        "the note stays out of the norm text: {para}"
    );
    let unknown =
        domain::get_modifications(&ctx, BGOE_VERSION, Some("art_999"), None).expect("runs");
    assert_eq!(unknown["error"], "not-found");
}

#[test]
fn list_annexes_names_path_eids_that_read_article_reads() {
    let ctx = fixture_ctx();
    let out = domain::list_annexes(&ctx, BGOE_VERSION, None).expect("runs");
    assert_eq!(out["kind"], "norm", "{out}");
    assert_eq!(out["total"], 1);
    let annex = &out["annexes"][0];
    assert_eq!(annex["doc_name"], "annex");
    assert_eq!(annex["eid_prefix"], "annex_u1");
    assert_eq!(annex["heading"], "Anhang");
    // X19.8: the row names the IRI from the component's OWN <FRBRWork> —
    // the recorded BGÖ component repeats the act's version IRI there.
    assert_eq!(
        annex["eli_work"], BGOE_VERSION,
        "the row names the IRI from the component's own identification block: {out}"
    );
    // X18.1: the BGÖ annex is a referral to the AS, not a text — the
    // row marks it as the stub it is.
    assert_eq!(
        annex["is_empty_stub"], true,
        "a referral stub is marked as one, not answered as an empty text: {annex}"
    );
    assert_eq!(annex["elements_total"], 1);
    assert_eq!(annex["elements_truncated"], false);
    let element = &annex["elements"][0];
    assert_eq!(element["eid"], "annex_u1/lvl_u1");
    assert_eq!(element["heading"], "Änderung bisherigen Rechts");
    // The path eId is directly readable.
    let text = domain::read_article(&ctx, BGOE_VERSION, element["eid"].as_str().unwrap(), None)
        .expect("runs");
    assert_eq!(text["kind"], "norm", "{text}");
    assert!(text["text"]
        .as_str()
        .unwrap()
        .contains("werden wie folgt geändert"));
}

#[test]
fn xml_tools_refuse_bad_versions_before_any_fetch() {
    let ctx = fixture_ctx();
    for bad in [
        "https://fedlex.data.admin.ch/eli/cc/2006/355",
        "https://evil.example/eli/cc/2006/355/20231101",
        "https://fedlex.data.admin.ch/eli/cc/2006/355/2023-11-01",
    ] {
        let out = domain::get_structure(&ctx, bad, None, None).expect("runs");
        assert_eq!(out["error"], "invalid-input", "{bad}: {out}");
    }
    // J13.1: «rm» is one of the vocabulary's five official languages —
    // whether a version carries XML in it is the GRAPH's answer, not a
    // rule of this server. What IS refused before any query is a
    // language the vocabulary does not define.
    let spanish = domain::search_text(&ctx, BGOE_VERSION, "x", Some("es"), None).expect("runs");
    assert_eq!(spanish["error"], "invalid-input", "{spanish}");
    assert!(
        spanish["detail"]
            .as_str()
            .unwrap()
            .contains("de|fr|it|en|rm"),
        "the refusal names the languages: {spanish}"
    );
    // J14.1: a version the graph lists with no XML manifestation is
    // refused with the reason travelling along — PDF-only, and the
    // tool that lists what there is.
    let pdf_only =
        domain::read_document(&ctx, &format!("{KVG}/19960101"), None, None, None).expect("runs");
    assert_eq!(pdf_only["error"], "not-found", "{pdf_only}");
    assert!(
        pdf_only["subject"]
            .as_str()
            .unwrap()
            .contains("no XML manifestation"),
        "{pdf_only}"
    );
    assert!(
        pdf_only["detail"].as_str().unwrap().contains("PDF-only"),
        "the reason travels with the refusal: {pdf_only}"
    );
    assert!(
        pdf_only["detail"]
            .as_str()
            .unwrap()
            .contains("list_expressions"),
        "{pdf_only}"
    );
}

/// J13.1 — the language of a manifestation is the graph's answer, not
/// a rule of this server: the recorded BGÖ 2023-11-01 carries XML in
/// English, read_article reads it, and the answer names the version and
/// the language it read. The five official languages are accepted;
/// anything else is refused before a request is made.
#[test]
fn read_article_reads_the_english_manifestation_and_names_its_language() {
    let ctx = fixture_ctx();
    let english = domain::read_article(&ctx, BGOE_VERSION, "art_6", Some("en")).expect("runs");
    assert_eq!(english["kind"], "norm", "{english}");
    assert_eq!(
        english["lang"], "en",
        "the answer says which manifestation it read"
    );
    assert_eq!(english["eli_version"], BGOE_VERSION);
    let text = english["text"].as_str().expect("text");
    assert!(
        text.contains("Principle of freedom of information"),
        "the English wording: {text}"
    );
    let german = domain::read_article(&ctx, BGOE_VERSION, "art_6", Some("de")).expect("runs");
    assert_eq!(german["lang"], "de");
    assert_ne!(
        german["text"], english["text"],
        "two languages, two wordings"
    );
    // A version WITHOUT XML in the language asked for is a not-found
    // with its ground — the KVG's 1996 consolidation is PDF-only.
    let pdf_only = domain::read_article(
        &ctx,
        "https://fedlex.data.admin.ch/eli/cc/1995/1328_1328_1328/19960101",
        "art_1",
        Some("de"),
    )
    .expect("runs");
    assert_eq!(pdf_only["error"], "not-found", "{pdf_only}");
    assert!(pdf_only["detail"].as_str().unwrap().contains("PDF-only"));
}

// --- BQ wave 1, B: the JOLux tools on their recorded keys ------------

#[test]
fn get_article_history_matches_the_element_exactly_and_joins_consolidations() {
    let ctx = fixture_ctx();
    // Art. 2 BGÖ: the graph carries NO impact on it (its 2009 FINMAG
    // change lives only in the text's change note). The vendored
    // primitive's substring match answered the impacts of art_20 …
    // art_23a here — the exact match answers an honest empty list
    // with the caveat.
    let none = domain::get_article_history(&ctx, BGOE, "art_2").expect("runs");
    assert_eq!(none["kind"], "norm", "{none}");
    assert_eq!(none["eid"], "art_2");
    assert_eq!(none["target"], format!("{BGOE}/art_2"));
    assert_eq!(none["total"], 0);
    assert_eq!(none["truncated"], false);
    // J6.4: the caveat is recency-specific — a subdivision-based
    // history degrades for amendments made since the 2023 system
    // change, and the answer points at the tool that still shows them.
    let note = none["completeness_note"].as_str().unwrap();
    assert!(note.contains("may be incomplete"), "{none}");
    assert!(
        note.contains("since the 2023 system change") && note.contains("fedlex.get_modifications"),
        "the caveat names the 2023 system change and points at the change notes: {none}"
    );
    assert_provenance_form(&none, "2026-08-21");
    // Art. 17: amended by oc/2023/584 in force 2023-11-01 — joined to
    // the 20231101 consolidation.
    let some = domain::get_article_history(&ctx, BGOE, "art_17").expect("runs");
    assert_eq!(some["kind"], "norm", "{some}");
    let impacts = some["impacts"].as_array().expect("impacts");
    assert_eq!(some["total"], impacts.len());
    assert!(!impacts.is_empty(), "{some}");
    let amendment = impacts
        .iter()
        .find(|i| i["date"] == "2023-11-01")
        .expect("the 2023-11-01 amendment of Art. 17");
    assert_eq!(amendment["version"], BGOE_VERSION);
    assert!(amendment["from"]
        .as_str()
        .expect("amending act")
        .contains("/eli/oc/2023/584"));
    assert_eq!(amendment["type_label"], "Änderung");
    let bad = domain::get_article_history(&ctx, BGOE, "art 2").expect("runs");
    assert_eq!(bad["error"], "invalid-input");
}

/// The provenance FORM of v0, unchanged: valid_as_of, transaction_time,
/// source — on every content answer, JOLux tools included.
fn assert_provenance_form(out: &serde_json::Value, valid_as_of: &str) {
    let provenance = &out["provenance"];
    assert_eq!(provenance["valid_as_of"], valid_as_of, "{out}");
    assert_eq!(provenance["transaction_time"], "2026-08-21");
    assert_eq!(
        provenance["source"],
        "fedlex.data.admin.ch/sparqlendpoint (live/base tier)"
    );
    assert_eq!(
        provenance.as_object().unwrap().len(),
        3,
        "the form has three fields"
    );
}

#[test]
fn get_subdivisions_is_a_gap_catalogue_with_eids() {
    let ctx = fixture_ctx();
    let out = domain::get_subdivisions(&ctx, BGOE).expect("runs");
    assert_eq!(out["kind"], "norm", "{out}");
    assert!(out["note"].as_str().unwrap().contains("gap catalogue"));
    // J17.3/J4.5b: the number depends on the walk, so the ANSWER names
    // the walk — a doc comment is not something a caller can read.
    assert!(
        out["walk"]
            .as_str()
            .is_some_and(|w| w.contains("transitive")),
        "{out}"
    );
    // J4.1: the note does not stop at the label — it hands the reader
    // on to the tool that reads the outline out of the XML.
    assert!(
        out["note"]
            .as_str()
            .unwrap()
            .contains("the outline is fedlex.get_structure"),
        "{out}"
    );
    let subs = out["subdivisions"].as_array().unwrap();
    assert_eq!(out["total"], subs.len());
    // J4.4: the kind of each subdivision is the graph's own vocabulary
    // IRI, never a word this server guessed.
    assert!(
        subs.iter().all(|s| s["type"].as_str().is_some_and(
            |t| t.starts_with("https://fedlex.data.admin.ch/vocabulary/subdivision-type/")
        )),
        "every row carries the type as a vocabulary IRI, never a guessed word: {out}"
    );
    assert!(
        !subs.is_empty(),
        "the amended BGÖ articles exist as subdivisions"
    );
    assert!(subs
        .iter()
        .any(|s| s["eid"].as_str().is_some_and(|e| e.starts_with("art_"))));
    // J4.2: the catalogue is far shorter than the act's own table of
    // contents — twelve amended elements against the whole outline.
    assert_eq!(out["total"], 12, "{out}");
    let outline = domain::get_structure(&ctx, BGOE_VERSION, None, None).expect("runs");
    assert!(
        outline["nodes_total"].as_u64().unwrap() > out["total"].as_u64().unwrap(),
        "the gap catalogue is not the act's table of contents: {out} vs {outline}"
    );
    assert_eq!(out["truncated"], false);
    assert_eq!(out["cap"], 500);
    assert_provenance_form(&out, "2026-08-21");
    // An act the graph does not know is not an act without
    // amendments: the profile decides, and it says not-found.
    let unknown = domain::get_subdivisions(&ctx, UNKNOWN).expect("runs");
    assert_eq!(unknown["error"], "not-found", "{unknown}");
    assert_eq!(unknown["subject"], UNKNOWN);
}

#[test]
fn get_taxonomy_classifies_with_notation_labels_and_branch() {
    let ctx = fixture_ctx();
    let out = domain::get_taxonomy(&ctx, KVG).expect("runs");
    assert_eq!(out["kind"], "norm", "{out}");
    let entries = out["entries"].as_array().unwrap();
    assert!(!entries.is_empty());
    let leaf = &entries[0];
    assert_eq!(
        leaf["notation"], "832.10",
        "the SR notation IS the taxonomy leaf"
    );
    assert!(leaf["labels"]["de"].as_str().is_some());
    assert!(leaf["labels"]["fr"].as_str().is_some());
    let branch = &out["branches"][0]["chain"].as_array().unwrap();
    assert!(
        branch.len() >= 2,
        "the chain climbs to the SR branch: {branch:?}"
    );
    assert_eq!(branch.last().unwrap()["uri"], leaf["uri"]);
    assert_eq!(
        branch[0]["notation"], "8",
        "root of the chain is the SR branch 8"
    );
    assert_eq!(out["truncated"], false);
    assert_provenance_form(&out, "2026-08-21");
    let bgoe = domain::get_taxonomy(&ctx, BGOE).expect("runs");
    assert_eq!(bgoe["entries"][0]["notation"], "152.3");
    assert_eq!(bgoe["branches"][0]["chain"][0]["notation"], "1");
    let unknown = domain::get_taxonomy(&ctx, UNKNOWN).expect("runs");
    assert_eq!(unknown["error"], "not-found", "{unknown}");
}

#[test]
fn list_expressions_shows_pdf_only_before_a_read() {
    let ctx = fixture_ctx();
    let recent = domain::list_expressions(&ctx, BGOE_VERSION).expect("runs");
    assert_eq!(recent["kind"], "norm", "{recent}");
    assert_eq!(recent["pdf_only"], false);
    let langs = recent["languages"].as_array().unwrap();
    let de = langs.iter().find(|l| l["lang"] == "de").expect("de");
    assert_eq!(de["xml_available"], true);
    assert!(de["formats"].as_array().unwrap().iter().any(|f| f == "xml"));
    assert!(langs.iter().any(|l| l["lang"] == "fr"));
    // J13.1: which languages a version carries is the GRAPH's answer —
    // the recorded BGÖ consolidation lists all five, English and
    // Romansh with XML like the other three.
    assert!(langs.iter().any(|l| l["lang"] == "it"), "{recent}");
    assert_eq!(
        langs.len(),
        5,
        "five language expressions recorded: {recent}"
    );
    let english = langs.iter().find(|l| l["lang"] == "en").expect("en");
    assert_eq!(
        english["xml_available"], true,
        "English is the GRAPH's answer, not a rule of the tool: {recent}"
    );
    let romansh = langs.iter().find(|l| l["lang"] == "rm").expect("rm");
    assert_eq!(romansh["xml_available"], true, "Romansh likewise: {recent}");

    assert_eq!(recent["xml_available"], true);
    // J19.5: listing walks past the expression to the manifestations —
    // and the tool that reads picks exactly one of them.
    let picked = domain::read_article(&ctx, BGOE_VERSION, "art_6", Some("de")).expect("runs");
    assert_eq!(
        picked["lang"], "de",
        "the reader picks ONE of the listed manifestations: {picked}"
    );
    assert!(
        picked["manifestation_url"]
            .as_str()
            .unwrap()
            .ends_with(".xml"),
        "{picked}"
    );
    let early = domain::list_expressions(&ctx, &format!("{KVG}/19960101")).expect("runs");
    assert_eq!(early["kind"], "norm", "{early}");
    assert_eq!(
        early["xml_available"], false,
        "1996: no XML — visible BEFORE reading"
    );
    // Recorded reality: the 1996 consolidation lists five language
    // expressions and NO manifestation file at all — not even a PDF.
    assert_eq!(early["no_manifestation_listed"], true);
    assert_eq!(
        early["pdf_only"], false,
        "pdf_only means «files, but no XML»"
    );
    assert_eq!(early["languages"].as_array().unwrap().len(), 5);
    assert!(early["note"].as_str().unwrap().contains("not-found"));
    assert_eq!(early["truncated"], false);
    assert_provenance_form(&recent, "2023-11-01");
    // And the read tools say the same thing, with the same ground:
    // the manifestation lookup is recorded (empty), so this is the
    // graph's answer, not a missing fixture.
    let read = domain::get_structure(&ctx, &format!("{KVG}/19960101"), None, None).expect("runs");
    assert_eq!(read["error"], "not-found", "{read}");
    assert!(read["detail"]
        .as_str()
        .unwrap()
        .contains("list_expressions"));
}

#[test]
fn resolve_vocabulary_label_works_by_label_by_iri_and_locally_for_languages() {
    let ctx = fixture_ctx();
    let by_label =
        domain::resolve_vocabulary_label(&ctx, "enforcement-status", "kraft", Some("de"))
            .expect("runs");
    assert_eq!(by_label["kind"], "hint", "{by_label}");
    let matches = by_label["matches"].as_array().unwrap();
    assert!(matches
        .iter()
        .any(|m| m["iri"] == "https://fedlex.data.admin.ch/vocabulary/enforcement-status/0"));
    let by_iri = domain::resolve_vocabulary_label(
        &ctx,
        "enforcement-status",
        "https://fedlex.data.admin.ch/vocabulary/enforcement-status/0",
        Some("de"),
    )
    .expect("runs");
    assert_eq!(by_iri["matches"][0]["label"], "In Kraft", "{by_iri}");
    assert_eq!(by_iri["answered_in"], "de", "the language that answered");
    // J5.4 — a concept the graph labels ONLY in French: the German
    // request is answered with the French label, and says so.
    let french_only = domain::resolve_vocabulary_label(
        &ctx,
        "legal-subject-theme-fr",
        "https://fedlex.data.admin.ch/vocabulary/legal-subject-theme-fr/22158",
        Some("de"),
    )
    .expect("runs");
    assert_eq!(french_only["matches"][0]["label"], "Code", "{french_only}");
    assert_eq!(french_only["answered_in"], "fr");
    assert_eq!(french_only["matches"][0]["lang"], "fr");
    // …and the label SEARCH: all twelve hits of «Code» in that scheme
    // carry no German label at all — before the fix every one of them
    // came back as an IRI with «label: null».
    let search =
        domain::resolve_vocabulary_label(&ctx, "legal-subject-theme-fr", "Code", Some("de"))
            .expect("runs");
    assert_eq!(search["returned"], 12, "{search}");
    assert_eq!(search["labels_filled"], 12, "twelve without a German label");
    assert!(
        search["matches"]
            .as_array()
            .unwrap()
            .iter()
            .all(|m| m["label"].is_string() && m["label_lang"] == "fr"),
        "every match carries a label and names its language: {search}"
    );
    assert_eq!(
        by_label["returned"],
        by_label["matches"].as_array().unwrap().len()
    );
    assert_eq!(by_label["truncated"], false, "three matches, cap 50");
    assert_provenance_form(&by_label, "2026-08-21");
    let foreign = domain::resolve_vocabulary_label(
        &ctx,
        "subdivision-type",
        "https://fedlex.data.admin.ch/vocabulary/enforcement-status/0",
        Some("de"),
    )
    .expect("runs");
    assert_eq!(foreign["error"], "invalid-input", "{foreign}");
    let language = domain::resolve_vocabulary_label(&ctx, "language", "fr", None).expect("runs");
    assert_eq!(
        language["matches"][0]["iri"],
        "http://publications.europa.eu/resource/authority/language/FRA"
    );
    let unknown_language =
        domain::resolve_vocabulary_label(&ctx, "language", "klingon", None).expect("runs");
    assert_eq!(unknown_language["error"], "not-found");
    let bad =
        domain::resolve_vocabulary_label(&ctx, "Enforcement Status", "x", None).expect("runs");
    assert_eq!(bad["error"], "invalid-input");
}

#[test]
fn find_related_topic_returns_capped_hints_by_eli_or_sr() {
    let ctx = fixture_ctx();
    let out = domain::find_related_topic(&ctx, Some(BGOE), None, Some(10)).expect("runs");
    assert_eq!(out["kind"], "hint", "{out}");
    let hits = out["hits"].as_array().unwrap();
    assert_eq!(hits.len(), 10, "capped at the limit");
    assert!(hits.iter().all(|h| h["eli"]
        .as_str()
        .unwrap()
        .starts_with("https://fedlex.data.admin.ch/eli/")));
    assert_eq!(out["returned"], 10);
    assert_eq!(out["limit"], 10);
    assert!(
        out.get("total").is_none(),
        "no total: the graph does not count"
    );
    assert_eq!(
        out["truncated"], true,
        "eleven rows were asked for and came back — the cap cut"
    );
    assert_provenance_form(&out, "2026-08-21");
    let neither = domain::find_related_topic(&ctx, None, None, None).expect("runs");
    assert_eq!(neither["error"], "invalid-input");
}

// --- the second recorded XML realities (KVG, LSV) --------------------

#[test]
fn the_kvg_outline_is_large_and_the_lsv_carries_annexes() {
    let ctx = fixture_ctx();
    let kvg_version = recorded_version_for(KVG).expect("a recorded KVG manifestation");
    let kvg = domain::get_structure(&ctx, &kvg_version, None, None).expect("runs");
    assert_eq!(kvg["kind"], "norm", "{kvg}");
    assert!(
        kvg["nodes_total"].as_u64().unwrap() > 100,
        "the KVG is a large act"
    );
    assert_eq!(kvg["truncated"], false, "the article skeleton fits the cap");
    let lsv_version = recorded_version_for("https://fedlex.data.admin.ch/eli/cc/1987/338")
        .expect("a recorded LSV manifestation");
    let annexes = domain::list_annexes(&ctx, &lsv_version, None).expect("runs");
    assert!(
        annexes["total"].as_u64().unwrap() >= 5,
        "the LSV has several annexes: {annexes}"
    );
    // X18.1: the other state of the same flag — every LSV annex carries
    // a body of its own, so none of them is a referral stub.
    assert!(
        annexes["annexes"]
            .as_array()
            .unwrap()
            .iter()
            .all(|a| a["is_empty_stub"] == false),
        "the LSV annexes carry bodies: {annexes}"
    );
    let first_eid = annexes["annexes"][0]["elements"][0]["eid"]
        .as_str()
        .expect("an annex element eId")
        .to_string();
    let read = domain::read_article(&ctx, &lsv_version, &first_eid, None).expect("runs");
    assert_eq!(read["kind"], "norm", "{read}");
    // Tables live in the annexes — the search sees their words.
    let tables = domain::search_text(&ctx, &lsv_version, "Belastungsgrenzwert", None, Some(5))
        .expect("runs");
    assert!(tables["total"].as_u64().unwrap() > 0, "{tables}");
    // X0.1: the XML tools are written against the consolidated
    // collection, and they say so — an abstract ELI is refused before
    // any query with the collection named in the refusal.
    let abstract_eli = domain::get_structure(&ctx, KVG, None, None).expect("runs");
    assert_eq!(abstract_eli["error"], "invalid-input", "{abstract_eli}");
    assert!(
        abstract_eli["detail"]
            .as_str()
            .unwrap()
            .contains("dated consolidation"),
        "the XML tools name the collection they are written against: {abstract_eli}"
    );
}

// --- BO′: the treaty reality (EMRK, SR 0.101) -------------------------

/// The EMRK chain, exactly as the graph answered it at recording: SR →
/// ELI → versions → governing version → its manifestations → Art. 3.
/// The test asserts BOTH branches precisely: if the governing version
/// lists an XML manifestation, Art. 3 is read as a norm; if it does
/// not, `read_article` answers not-found with the PDF-only ground —
/// the reality is recorded, never worked around.
#[test]
fn the_emrk_chain_is_recorded_reality_treaty_manifestations_included() {
    let ctx = fixture_ctx();
    let emrk = domain::resolve_sr(&ctx, EMRK_SR).expect("runs");
    assert_eq!(emrk["kind"], "norm", "{emrk}");
    assert_eq!(emrk["sr"], EMRK_SR);
    let eli = emrk["eli"].as_str().expect("eli").to_string();
    assert!(eli.starts_with("https://fedlex.data.admin.ch/eli/cc/"));
    assert!(
        emrk["title"]["de"]
            .as_str()
            .unwrap()
            .contains("Menschenrechte"),
        "{emrk}"
    );
    let versions = domain::list_versions(&ctx, &eli).expect("runs");
    assert!(!versions["versions"].as_array().unwrap().is_empty());
    let governing = domain::resolve_consolidation_at(&ctx, &eli, "2026-08-29").expect("runs");
    let version = governing["eli_version"]
        .as_str()
        .expect("version")
        .to_string();
    let expressions = domain::list_expressions(&ctx, &version).expect("runs");
    assert_eq!(expressions["kind"], "norm", "{expressions}");
    // J12.4: the recorded EMRK consolidation carries XML, so the loop
    // reaches the treaty's norm text exactly as it does a domestic
    // act's — the branch below is decided, not hoped for.
    assert_eq!(
        expressions["xml_available"], true,
        "the recorded EMRK consolidation carries XML — the treaty reads like a domestic act: {expressions}"
    );
    let article = domain::read_article(&ctx, &version, "art_3", Some("de")).expect("runs");
    if expressions["xml_available"] == true {
        assert_eq!(article["kind"], "norm", "{article}");
        assert_eq!(article["eid"], "art_3");
        assert!(!article["text"].as_str().unwrap().is_empty());
    } else {
        assert_eq!(article["error"], "not-found", "{article}");
        assert!(article["subject"]
            .as_str()
            .unwrap()
            .contains("no XML manifestation"));
        assert!(article["detail"].as_str().unwrap().contains("PDF-only"));
    }
}

// --- BO′: the manifestation cache, proven by counting --------------

fn counting_ctx(
    cache: ManifestationCache,
) -> (
    Ctx,
    std::sync::Arc<std::sync::atomic::AtomicUsize>,
    std::sync::Arc<std::sync::atomic::AtomicUsize>,
) {
    let (backend, selects, fetches) = Backend::counting(
        fixtures_dir(),
        cache,
        vec![
            "2026-08-29T10:00:00Z".into(),
            "2026-08-29T10:05:00Z".into(),
            "2026-08-29T10:10:00Z".into(),
            "2026-08-29T10:15:00Z".into(),
        ],
    );
    let ctx = Ctx {
        backend,
        today: "2026-08-29".into(),
    };
    (ctx, selects, fetches)
}

/// Two reads of the same version + language cost ONE fetch (and ONE
/// resolution query); the second answer says `served: cache` and
/// keeps the FIRST retrieval moment as its transaction_time instead
/// of claiming the later clock value — and every other XML tool on
/// that version rides the same line.
#[test]
fn two_reads_of_one_version_fetch_the_manifestation_once() {
    use std::sync::atomic::Ordering;
    let (ctx, selects, fetches) = counting_ctx(ManifestationCache::default_sized());
    let first = domain::read_article(&ctx, BGOE_VERSION, "art_6", Some("de")).expect("runs");
    assert_eq!(first["kind"], "norm", "{first}");
    assert_eq!(first["provenance"]["served"], "live");
    assert_eq!(
        first["provenance"]["transaction_time"],
        "2026-08-29T10:00:00Z"
    );
    assert_eq!(first["provenance"]["valid_as_of"], "2023-11-01");
    assert_eq!(fetches.load(Ordering::SeqCst), 1);
    assert_eq!(selects.load(Ordering::SeqCst), 1);
    // J2.1: that one select is the FRBR resolution, and it is walked
    // INTO the abstract act — the direction lives in the query text,
    // which the counting double keeps.
    assert!(
        ctx.backend.seen_queries()[0].contains("?cons jolux:isMemberOf <"),
        "the FRBR chain is walked INTO the abstract act: {:?}",
        ctx.backend.seen_queries()
    );

    let second = domain::read_article(&ctx, BGOE_VERSION, "art_17", Some("de")).expect("runs");
    assert_eq!(second["kind"], "norm");
    assert_eq!(second["provenance"]["served"], "cache");
    assert_eq!(
        second["provenance"]["transaction_time"], "2026-08-29T10:00:00Z",
        "a cache hit keeps the ORIGINAL retrieval moment"
    );
    assert_eq!(fetches.load(Ordering::SeqCst), 1, "one fetch for two reads");
    assert_eq!(
        selects.load(Ordering::SeqCst),
        1,
        "no second resolution query either"
    );

    // The outline and the search ride the same cache line.
    let outline = domain::get_structure(&ctx, BGOE_VERSION, Some("de"), None).expect("runs");
    assert_eq!(outline["provenance"]["served"], "cache");
    let hits = domain::search_text(&ctx, BGOE_VERSION, "Zugang", None, None).expect("runs");
    assert_eq!(hits["provenance"]["served"], "cache");
    assert_eq!(fetches.load(Ordering::SeqCst), 1);

    // Another language is another key: the French text is fetched
    // once more, and stamped with ITS own moment.
    let french = domain::read_article(&ctx, BGOE_VERSION, "art_6", Some("fr")).expect("runs");
    assert_eq!(french["kind"], "norm", "{french}");
    assert_eq!(french["provenance"]["served"], "live");
    assert_eq!(
        french["provenance"]["transaction_time"],
        "2026-08-29T10:05:00Z"
    );
    assert_eq!(fetches.load(Ordering::SeqCst), 2);

    // The v0 provenance fields are still exactly there — the form
    // was extended by one field, not changed.
    let provenance = first["provenance"].as_object().unwrap();
    for field in ["valid_as_of", "transaction_time", "source", "served"] {
        assert!(provenance.contains_key(field), "{field}");
    }
    assert_eq!(provenance.len(), 4);
}

/// The cache is bounded: beyond its entry cap the least recently used
/// line goes, and the next read of that version fetches again.
#[test]
fn the_cache_evicts_beyond_its_cap_and_fetches_again() {
    use std::sync::atomic::Ordering;
    let (ctx, _, fetches) = counting_ctx(ManifestationCache::new(1 << 24, 1));
    domain::read_article(&ctx, BGOE_VERSION, "art_6", Some("de")).expect("runs");
    domain::read_article(&ctx, BGOE_VERSION, "art_6", Some("fr")).expect("runs");
    assert_eq!(fetches.load(Ordering::SeqCst), 2);
    // The German line was evicted by the French one (cap: one entry).
    let again = domain::read_article(&ctx, BGOE_VERSION, "art_6", Some("de")).expect("runs");
    assert_eq!(again["provenance"]["served"], "live");
    assert_eq!(
        again["provenance"]["transaction_time"],
        "2026-08-29T10:10:00Z"
    );
    assert_eq!(fetches.load(Ordering::SeqCst), 3);

    // The byte cap: a manifestation larger than the cap is served
    // live every time and never displaces anything.
    let (ctx, _, fetches) = counting_ctx(ManifestationCache::new(50_000, 8));
    domain::read_article(&ctx, BGOE_VERSION, "art_6", Some("de")).expect("runs"); // 42 KB fits
    let kvg_version = recorded_version_for(KVG).expect("a recorded KVG manifestation");
    let big = domain::read_article(&ctx, &kvg_version, "art_1", Some("de")).expect("runs");
    assert_eq!(big["provenance"]["served"], "live", "{big}");
    let big_again = domain::read_article(&ctx, &kvg_version, "art_1", Some("de")).expect("runs");
    assert_eq!(
        big_again["provenance"]["served"], "live",
        "433 KB > 50 KB: never cached"
    );
    assert_eq!(fetches.load(Ordering::SeqCst), 3);
    let small_again = domain::read_article(&ctx, BGOE_VERSION, "art_6", Some("de")).expect("runs");
    assert_eq!(
        small_again["provenance"]["served"], "cache",
        "the small line survived"
    );
    assert_eq!(fetches.load(Ordering::SeqCst), 3);
}

/// The Fixtures backend never caches and says `served: fixture`.
#[test]
fn fixtures_are_served_as_fixtures_never_as_live_or_cache() {
    let ctx = fixture_ctx();
    let out = domain::read_article(&ctx, BGOE_VERSION, "art_6", Some("de")).expect("runs");
    assert_eq!(out["provenance"]["served"], "fixture");
    assert_eq!(out["provenance"]["transaction_time"], "2026-08-21");
    let again = domain::read_article(&ctx, BGOE_VERSION, "art_6", Some("de")).expect("runs");
    assert_eq!(again["provenance"]["served"], "fixture");
    // X0.3: the other side of the house answers in its own form — a
    // graph answer names its source and never a `served` state.
    let graph = domain::get_law_metadata(&ctx, KVG, None).expect("runs");
    assert!(
        graph["provenance"].get("served").is_none(),
        "a graph answer names source, never served: {graph}"
    );
    assert_provenance_form(&graph, graph["provenance"]["valid_as_of"].as_str().unwrap());
}

// --- BR wave 2: the research-critical tools ---------------------------

/// LSV (Lärmschutz-Verordnung) governing version recorded at BQ —
/// annexes with limit-value tables.
const LSV_VERSION: &str = "https://fedlex.data.admin.ch/eli/cc/1987/338_338_338/20260401";
/// The BGÖ consolidation BEFORE the 2023-11-01 amendment (Art. 17 new
/// wording, Art. 23a inserted) — recorded at BR for compare_versions.
const BGOE_OLDER: &str = "https://fedlex.data.admin.ch/eli/cc/2006/355/20230901";
/// EnG (Energiegesetz) — the genesis example with consultations.
const ENG: &str = "https://fedlex.data.admin.ch/eli/cc/2017/762";

/// The citation table of parse_reference: text → what a machine must
/// read out of it. Acts are asserted by SR number (the abbreviation
/// fixtures decide the ELI).
fn reference_table() -> Vec<(&'static str, serde_json::Value)> {
    vec![
        (
            "Art. 25 Abs. 1 USG",
            serde_json::json!({"kind":"article","abbreviation":"USG","sr":"814.01","article":"25","paragraph":"1","eid":"art_25/para_1"}),
        ),
        (
            "Art. 7 Abs. 1 lit. b LSV",
            serde_json::json!({"kind":"article","abbreviation":"LSV","sr":"814.41","article":"7","paragraph":"1","letter":"b","eid":"art_7/para_1/lbl_b"}),
        ),
        (
            "Anhang 3 Ziff. 2 LSV",
            serde_json::json!({"kind":"annex","abbreviation":"LSV","sr":"814.41","annex":"3","number":"2","eid":null,"annex_hint":"annex_3"}),
        ),
        (
            "Art. 6 BGÖ",
            serde_json::json!({"kind":"article","abbreviation":"BGÖ","sr":"152.3","article":"6","eid":"art_6"}),
        ),
        (
            "Artikel 23a Absatz 2 BGÖ",
            serde_json::json!({"kind":"article","abbreviation":"BGÖ","sr":"152.3","article":"23a","paragraph":"2","eid":"art_23_a/para_2"}),
        ),
        (
            "Art. 3 Abs. 1 Bst. c Ziff. 2 DSG",
            serde_json::json!({"kind":"article","abbreviation":"DSG","sr":"235.1","article":"3","paragraph":"1","letter":"c","number":"2","eid":"art_3/para_1/lbl_c/lbl_2"}),
        ),
        (
            "Art. 41 ff. OR",
            serde_json::json!({"kind":"article","abbreviation":"OR","sr":"220","article":"41","following":true,"eid":"art_41"}),
        ),
        (
            "Art. 25a Abs. 1bis KVG",
            serde_json::json!({"kind":"article","abbreviation":"KVG","sr":"832.10","article":"25a","paragraph":"1bis","eid":"art_25_a/para_1_bis"}),
        ),
        (
            "SR 832.10",
            serde_json::json!({"kind":"sr","sr":"832.10","eid":null}),
        ),
        (
            "AS 2020 752",
            serde_json::json!({"kind":"as","memorial":"AS 2020 752","act":null,"eid":null}),
        ),
        (
            "Art. 5 Abs. 2 XYZ",
            serde_json::json!({"kind":"article","abbreviation":"XYZ","unresolved":true,"act":null,"eid":"art_5/para_2"}),
        ),
        (
            "Art. 12",
            serde_json::json!({"kind":"article","act":null,"unresolved":false,"article":"12","eid":"art_12"}),
        ),
        (
            "Anhang 1 LSV",
            serde_json::json!({"kind":"annex","abbreviation":"LSV","sr":"814.41","annex":"1","annex_hint":"annex_1"}),
        ),
        (
            "Art. 4 Abs. 2 LSV",
            serde_json::json!({"kind":"article","abbreviation":"LSV","sr":"814.41","eid":"art_4/para_2"}),
        ),
    ]
}

/// Records the BR keys (abbreviations for the citation table, the
/// older BGÖ consolidation, the formal citation graph, explore_node,
/// treaties, the genesis of the nDSG, the AS chain of the BGÖ):
/// `cargo test --test e2e record_fixtures_br -- --ignored --nocapture --test-threads 1`
#[test]
#[ignore = "hits the live endpoint once per key; run deliberately"]
fn record_fixtures_br() {
    let ctx = Ctx {
        backend: Backend::recording(FEDLEX_ENDPOINT, fixtures_dir()),
        today: "2026-08-21".into(),
    };
    record_fixtures_br_with(&ctx);
}

/// The EnG (Energiegesetz, `eli/cc/2017/762`) — upstream's verified
/// example of a draft WITH consultations and Federal Gazette documents;
/// recorded so the genesis tools show a non-empty answer beside the
/// nDSG's honest zeros:
/// `cargo test --test e2e record_fixtures_br_eng -- --ignored --nocapture --test-threads 1`
#[test]
#[ignore = "hits the live endpoint once per key; run deliberately"]
fn record_fixtures_br_eng() {
    let ctx = Ctx {
        backend: Backend::recording(FEDLEX_ENDPOINT, fixtures_dir()),
        today: "2026-08-21".into(),
    };
    record_fixtures_br_eng_with(&ctx);
}

fn record_fixtures_br_eng_with(ctx: &Ctx) {
    let drafts = domain::get_drafts(ctx, ENG).expect("record");
    println!("get_drafts EnG: {}", drafts["total"]);
    let consultations =
        domain::get_consultations(ctx, Some(ENG), None, None, None).expect("record");
    println!("get_consultations EnG: {}", consultations["total"]);
    if let Some(c) = consultations["consultations"][0]["consultation"].as_str() {
        let docs = domain::get_consultation_documents(ctx, c).expect("record");
        println!("get_consultation_documents EnG: {}", docs["total"]);
    }
    let fga = domain::get_fga_documents(ctx, ENG).expect("record");
    println!("get_fga_documents EnG: {}", fga["total"]);
}

fn record_fixtures_br_with(ctx: &Ctx) {
    // Abbreviations of the citation table (one exact pre-query each,
    // keys shared with search_law) — through parse_reference itself.
    for (text, _) in reference_table() {
        let out = domain::parse_reference(ctx, text).expect("record");
        println!(
            "parse_reference {text}: {}",
            out["references"][0]["act"]["sr"]
        );
    }
    domain::parse_reference(ctx, "Art. 8 EMRK i.V.m. Art. 36 BV").expect("record");
    // The older BGÖ consolidation, for compare_versions.
    let older = domain::read_article(ctx, BGOE_OLDER, "art_17", Some("de")).expect("record");
    assert!(older.get("error").is_none(), "{older}");
    // The formal citation graph and the node view of the BGÖ.
    for direction in ["cites", "cited_by"] {
        let out = domain::get_citations(ctx, BGOE, direction).expect("record");
        println!("get_citations {direction}: {}", out["total"]);
    }
    let node = domain::explore_node(ctx, BGOE, Some(20)).expect("record");
    println!(
        "explore_node: out {} in {}",
        node["outgoing"].as_array().map_or(0, Vec::len),
        node["incoming"].as_array().map_or(0, Vec::len)
    );
    // Treaties: the EMRK by a word of its title, then its profile.
    let treaties =
        domain::find_treaties(ctx, Some("Menschenrechte"), None, None, Some(10)).expect("record");
    println!(
        "find_treaties: {} hits, first {}",
        treaties["returned"], treaties["hits"][0]["process"]
    );
    if let Some(process) = treaties["hits"][0]["process"].as_str() {
        let info = domain::get_treaty_info(ctx, process, Some("de")).expect("record");
        println!("get_treaty_info: {}", info["treaty"]["title"]);
    }
    // The genesis of the nDSG: drafts → consultations → documents; the
    // Federal Gazette documents.
    let drafts = domain::get_drafts(ctx, NDSG).expect("record");
    println!("get_drafts nDSG: {}", drafts["total"]);
    let consultations =
        domain::get_consultations(ctx, Some(NDSG), None, None, None).expect("record");
    println!("get_consultations nDSG: {}", consultations["total"]);
    if let Some(c) = consultations["consultations"][0]["consultation"].as_str() {
        let docs = domain::get_consultation_documents(ctx, c).expect("record");
        println!("get_consultation_documents: {}", docs["total"]);
    }
    let fga = domain::get_fga_documents(ctx, NDSG).expect("record");
    println!("get_fga_documents nDSG: {}", fga["total"]);
    // The Official Compilation chain of the BGÖ.
    let oc = domain::get_oc_act(ctx, BGOE).expect("record");
    println!("get_oc_act BGÖ: {}", oc["oc"]);
    if let Some(oc_eli) = oc["oc"].as_str() {
        let memorial = domain::get_memorial(ctx, oc_eli, Some(20)).expect("record");
        println!(
            "get_memorial: {} ({} acts)",
            memorial["memorial"], memorial["returned"]
        );
    }
    let unknown = domain::get_oc_act(ctx, UNKNOWN).expect("record");
    println!("get_oc_act unknown: {}", unknown["error"]);
}

/// The old Energy Act (SR 730.0 in its 1998 shape) — repealed on
/// 2018-01-01, twelve consolidations: the recorded case for the
/// in-force rule (BV addendum).
const ENG_REPEALED: &str = "https://fedlex.data.admin.ch/eli/cc/1999/27";
/// A data-protection act the graph knows by title and never
/// consolidated: no status, no date, no consolidation — the recorded
/// case for «an empty version list is an answer» (J3.3).
const DSG_STUB: &str = "https://fedlex.data.admin.ch/eli/cc/2020/2930_cc";

/// BV addendum: re-records the keys whose QUERY changed (the status
/// label and the two end dates joined into get_law_metadata and
/// resolve_sr) and records the new ones (the act-level in-force check,
/// the repealed act, the never-consolidated act) — one polite request
/// per key:
/// `cargo test --test e2e record_fixtures_bv -- --ignored --nocapture --test-threads 1`
#[test]
#[ignore = "hits the live endpoint once per key; run deliberately"]
fn record_fixtures_bv() {
    let ctx = Ctx {
        backend: Backend::recording(FEDLEX_ENDPOINT, fixtures_dir()),
        today: "2026-08-21".into(),
    };
    let kvg_version = recorded_version_for(KVG).expect("a recorded KVG manifestation");
    let _ = kvg_version;
    // The metadata query gained statusLabel, noLonger and endApp.
    for eli in [
        KVG,
        "https://fedlex.data.admin.ch/eli/cc/1987/338_338_338",
        "https://fedlex.data.admin.ch/eli/cc/1974/2151_2151_2151",
        NDSG,
        UNKNOWN,
        ENG_REPEALED,
        DSG_STUB,
    ] {
        let out = domain::get_law_metadata(&ctx, eli, None).expect("record");
        println!(
            "get_law_metadata {eli}: status_label={} in_force={} dates={}",
            out["status_label"], out["in_force"], out["dates"]
        );
    }
    // The candidates query gained the same joins.
    for sr in ["832.10", "814.41", "0.101", "999.99"] {
        let out = domain::resolve_sr(&ctx, sr).expect("record");
        println!("resolve_sr {sr}: {}", out["eli"]);
    }
    // The act-level in-force check (vendored JLX-TMP-03 over the bridge).
    for eli in [KVG, ENG_REPEALED, DSG_STUB, UNKNOWN] {
        let out = domain::check_in_force(&ctx, eli, "2026-08-29").expect("record");
        println!(
            "check_in_force {eli}: in_force={} dates={} status={}",
            out["in_force"], out["dates"], out["status_label"]
        );
    }
    // The never-consolidated act's (empty) version list, and the
    // unknown ELI that must stay a not-found.
    for eli in [DSG_STUB, UNKNOWN] {
        let versions = domain::list_versions(&ctx, eli).expect("record");
        println!(
            "list_versions {eli}: total={} error={}",
            versions["total"], versions["error"]
        );
    }
}

/// Records the keys Part A′ needs — the vocabulary labels of a
/// concept (all languages in one query), a concept the graph labels
/// ONLY in French, the label search that has to fall back, and the
/// English manifestation of the recorded BGÖ version (J13.1: the graph
/// decides which languages a version carries):
/// `cargo test --test e2e record_fixtures_bv_prime -- --ignored --nocapture --test-threads 1`
#[test]
#[ignore = "hits the live endpoint once per key; run deliberately"]
fn record_fixtures_bv_prime() {
    let ctx = Ctx {
        backend: Backend::recording(FEDLEX_ENDPOINT, fixtures_dir()),
        today: "2026-08-21".into(),
    };
    // J5.4 — the labels of a concept, in every language it has.
    for (scheme, iri) in [
        (
            "enforcement-status",
            "https://fedlex.data.admin.ch/vocabulary/enforcement-status/0",
        ),
        (
            "legal-subject-theme-fr",
            "https://fedlex.data.admin.ch/vocabulary/legal-subject-theme-fr/22158",
        ),
    ] {
        let out = domain::resolve_vocabulary_label(&ctx, scheme, iri, Some("de")).expect("record");
        println!(
            "resolve_vocabulary_label {iri}: label={} answered_in={}",
            out["matches"][0]["label"], out["answered_in"]
        );
    }
    // J5.4 — the label SEARCH that finds nothing in German and has to
    // ask the other languages (one key per language until one answers).
    let search =
        domain::resolve_vocabulary_label(&ctx, "legal-subject-theme-fr", "Code", Some("de"))
            .expect("record");
    println!(
        "resolve_vocabulary_label search: returned={} labels_filled={} matches={}",
        search["returned"], search["labels_filled"], search["matches"]
    );
    // J5.4 — a label search that finds NOTHING in the language asked
    // for: «Riaccettazione» is an Italian-only theme label, so de, en
    // and fr answer empty and it answers (one key per language).
    let fallback = domain::resolve_vocabulary_label(
        &ctx,
        "legal-subject-theme-it",
        "Riaccettazione",
        Some("de"),
    )
    .expect("record");
    println!(
        "resolve_vocabulary_label fallback: returned={} labels_filled={} matches={}",
        fallback["returned"], fallback["labels_filled"], fallback["matches"]
    );
    // J13.1 — the English XML of the recorded BGÖ version.
    let english = domain::read_article(&ctx, BGOE_VERSION, "art_6", Some("en")).expect("record");
    println!(
        "read_article en: lang={} error={}",
        english["lang"], english["error"]
    );
}

/// J9.3 — the Federal Gazette, the Official Compilation, its memorials
/// and the drafts are structurally unlike the classified compilation:
/// they carry no consolidations, no impacts, no citation edges. The
/// tools that answer versions, history or citations refuse them BEFORE
/// any request and name the tool that does answer for them — an empty
/// list would look like an answer.
#[test]
fn the_version_history_and_citation_tools_refuse_another_collection() {
    let ctx = fixture_ctx();
    let cases = [
        (
            "https://fedlex.data.admin.ch/eli/fga/2013/1477",
            "Federal Gazette",
        ),
        (
            "https://fedlex.data.admin.ch/eli/oc/2006/355",
            "Official Compilation",
        ),
        (
            "https://fedlex.data.admin.ch/eli/collection/oc/2006/24",
            "memorial",
        ),
        (
            "https://fedlex.data.admin.ch/eli/dl/proj/8022/0491",
            "draft or a consultation",
        ),
    ];
    for (iri, what) in cases {
        let answers = [
            domain::list_versions(&ctx, iri).expect("runs"),
            domain::resolve_consolidation_at(&ctx, iri, "2026-08-29").expect("runs"),
            domain::get_article_history(&ctx, iri, "art_1").expect("runs"),
            domain::get_citations(&ctx, iri, "cites").expect("runs"),
            domain::get_subdivisions(&ctx, iri).expect("runs"),
            domain::check_in_force(&ctx, iri, "2026-08-29").expect("runs"),
        ];
        for out in answers {
            assert_eq!(out["error"], "invalid-input", "{iri}: {out}");
            let detail = out["detail"].as_str().expect("a detail");
            assert!(detail.contains(what), "{iri}: {detail}");
            assert!(
                detail.contains("fedlex."),
                "the refusal names the tool that does answer: {detail}"
            );
        }
    }
    // …and the act of the classified compilation is answered as before.
    let ok = domain::list_versions(&ctx, BGOE).expect("runs");
    assert!(ok.get("error").is_none(), "{ok}");
}

/// X0.2 — every file of the corpus parses, so a parse failure is an
/// UPSTREAM fault and is reported as one, never swallowed into an
/// empty answer. The case does not exist in the recorded fixtures (all
/// of them are real, well-formed manifestations), so the test builds
/// the double itself: the recorded resolution answer beside a
/// manifestation body that is not Akoma Ntoso.
#[test]
fn a_manifestation_that_does_not_parse_is_an_upstream_fault() {
    let dir = std::env::temp_dir().join(format!("oh-fedlex-parse-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    let select = format!("manifestation:{BGOE_VERSION}:de");
    let body = format!("manifestation:xml:{BGOE_VERSION}:de");
    std::fs::copy(
        fixtures_dir().join(oh_mcp_fedlex::backend::fixture_file_name(&select)),
        dir.join(oh_mcp_fedlex::backend::fixture_file_name(&select)),
    )
    .expect("the recorded resolution answer");
    std::fs::write(
        dir.join(oh_mcp_fedlex::backend::fixture_file_name(&body))
            .with_extension("xml"),
        "<akomaNtoso><act><body><article>unclosed",
    )
    .expect("the broken body");
    let ctx = Ctx {
        backend: Backend::Fixtures { dir: dir.clone() },
        today: "2026-08-21".into(),
    };
    let out = domain::read_article(&ctx, BGOE_VERSION, "art_6", Some("de")).expect("runs");
    assert_eq!(out["error"], "upstream-unavailable", "{out}");
    assert!(
        out["detail"].as_str().unwrap().contains("does not parse"),
        "the fault is named as the parser's: {out}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// X6.1 — the text elements the corpus writes are not all the ones the
/// vendored writer separates: `<br/>` produces NO separator, so two
/// printed lines run together in the answer. The KVG's Art. 55a writes
/// its heading over two lines and comes out as «Ärztinnen,die». The
/// rule is NOT honoured; this test holds the consequence still, and a
/// fix has to change exactly what it asserts.
#[test]
fn a_line_break_runs_two_lines_together_in_the_answer() {
    let ctx = fixture_ctx();
    // The LSV's limit-value table writes «Planungswert<br/>Lr in dB(A)»
    // in its header cells — the break carries the second line, and the
    // extraction drops it without a separator.
    let tables = domain::extract_tables(&ctx, LSV_VERSION, Some("annex_3/lvl_u1/lvl_2"), None)
        .expect("runs");
    let header = tables["tables"][0]["header"].as_array().expect("header");
    assert_eq!(header[1], "PlanungswertLr in dB(A)", "{header:?}");
    assert_eq!(header[3], "ImmissionsgrenzwertLr in dB(A)", "{header:?}");
    // …so a quote of the PRINTED wording cannot be verified, while the
    // run-together form can. That is the consequence, in one pair.
    let printed = "Planungswert Lr in dB(A)";
    let out = domain::check_quote(
        &ctx,
        LSV_VERSION,
        "annex_3/lvl_u1/lvl_2",
        printed,
        Some("de"),
    )
    .expect("runs");
    assert_eq!(out["verified"], false, "{out}");
    let extracted = "PlanungswertLr in dB(A)";
    let out = domain::check_quote(
        &ctx,
        LSV_VERSION,
        "annex_3/lvl_u1/lvl_2",
        extracted,
        Some("de"),
    )
    .expect("runs");
    assert_eq!(out["verified"], true, "what the tools DO carry: {out}");
}

/// BV A′ point 5 — every fixture is indexed WITH the day it was
/// recorded, and every indexed file exists: a recorded answer without
/// its moment is an undated claim, and a file nobody indexes is a
/// claim nobody can check.
#[test]
fn every_fixture_is_indexed_with_the_day_it_was_recorded() {
    let keys = recorded_keys();
    assert!(keys.len() > 90, "the index carries the recorded keys");
    let dir = fixtures_dir();
    for (file, key, recorded) in &keys {
        assert!(!key.is_empty(), "a line without a key: {file}");
        let day = recorded.strip_prefix('~').unwrap_or(recorded);
        assert_eq!(day.len(), 10, "«{recorded}» is no date ({key})");
        assert!(
            day.chars().all(|c| c.is_ascii_digit() || c == '-'),
            "«{recorded}» is no date ({key})"
        );
        assert!(
            dir.join(file).exists(),
            "the index names {file} for «{key}», and it is not there"
        );
    }
    // …and nothing lies in the directory that the index does not name.
    let indexed: std::collections::BTreeSet<&str> =
        keys.iter().map(|(f, _, _)| f.as_str()).collect();
    for entry in std::fs::read_dir(&dir).expect("fixtures dir") {
        let entry = entry.expect("dir entry");
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == "INDEX.txt" {
            continue;
        }
        assert!(
            indexed.contains(name.as_str()),
            "{name} is a fixture nobody indexes"
        );
    }
}

/// BV A′ point 5 — what the two recording passes actually COST. The
/// recording backend writes one file per KEY, and a tool that calls
/// another tool makes requests the key list never shows; the counting
/// double replays the very same call sequence over the recorded
/// fixtures and counts every select and every fetch, nested calls
/// included. The numbers here are the ones the report carries.
#[test]
fn the_bv_recording_passes_cost_what_the_report_says() {
    use std::sync::atomic::Ordering;
    let (ctx, selects, fetches) = counting_ctx(ManifestationCache::default_sized());
    // — the BV pass, call for call —
    for eli in [
        KVG,
        "https://fedlex.data.admin.ch/eli/cc/1987/338_338_338",
        "https://fedlex.data.admin.ch/eli/cc/1974/2151_2151_2151",
        NDSG,
        UNKNOWN,
        ENG_REPEALED,
        DSG_STUB,
    ] {
        domain::get_law_metadata(&ctx, eli, None).expect("runs");
    }
    for sr in ["832.10", "814.41", "0.101", "999.99"] {
        domain::resolve_sr(&ctx, sr).expect("runs");
    }
    for eli in [KVG, ENG_REPEALED, DSG_STUB, UNKNOWN] {
        domain::check_in_force(&ctx, eli, "2026-08-29").expect("runs");
    }
    for eli in [DSG_STUB, UNKNOWN] {
        domain::list_versions(&ctx, eli).expect("runs");
    }
    let bv_selects = selects.load(Ordering::SeqCst);
    let bv_fetches = fetches.load(Ordering::SeqCst);
    // The pass writes ONE FILE per distinct key while making one
    // request per select — the difference is what the nested calls cost.
    let bv_keys: std::collections::BTreeSet<String> = ctx.backend.seen_keys().into_iter().collect();
    // — the A′ pass, call for call —
    for (scheme, iri) in [
        (
            "enforcement-status",
            "https://fedlex.data.admin.ch/vocabulary/enforcement-status/0",
        ),
        (
            "legal-subject-theme-fr",
            "https://fedlex.data.admin.ch/vocabulary/legal-subject-theme-fr/22158",
        ),
    ] {
        domain::resolve_vocabulary_label(&ctx, scheme, iri, Some("de")).expect("runs");
    }
    domain::resolve_vocabulary_label(&ctx, "legal-subject-theme-fr", "Code", Some("de"))
        .expect("runs");
    domain::resolve_vocabulary_label(&ctx, "legal-subject-theme-it", "Riaccettazione", Some("de"))
        .expect("runs");
    domain::read_article(&ctx, BGOE_VERSION, "art_6", Some("en")).expect("runs");
    let all_selects = selects.load(Ordering::SeqCst);
    let all_fetches = fetches.load(Ordering::SeqCst);
    let all_keys: std::collections::BTreeSet<String> =
        ctx.backend.seen_keys().into_iter().collect();
    println!(
        "BV: {bv_selects} selects over {} keys + {bv_fetches} fetches; \
         A′ adds {} selects over {} keys + {} fetches\nBV keys: {:#?}",
        bv_keys.len(),
        all_selects - bv_selects,
        all_keys.len() - bv_keys.len(),
        all_fetches - bv_fetches,
        bv_keys
    );
    // The BV pass wrote 18 fixture files and made MORE requests than
    // that: get_law_metadata inside resolve_sr and check_in_force, the
    // consolidation lookup inside check_in_force, the version list
    // inside resolve_consolidation_at.
    assert_eq!(bv_selects, 28, "BV selects");
    assert_eq!(
        bv_keys.len(),
        19,
        "…over this many distinct keys — the pass writes one file per key, \
         and one of them (list_versions:<KVG>) already existed"
    );
    assert_eq!(bv_fetches, 0, "the BV pass fetched no manifestation");
    assert_eq!(
        all_selects - bv_selects,
        7,
        "A′ selects (7 of the 8 A′ keys)"
    );
    assert_eq!(
        all_fetches - bv_fetches,
        1,
        "A′ fetched the English XML once"
    );
}

/// J3.5's census, measured by the suite instead of by hand (BY′): how
/// many recorded German act titles name their subject with «über» or
/// «betreffend», and how many carry the promulgation date in front of
/// it. The rule's figure comes from THIS, so it cannot drift from the
/// fixtures it describes — the precedent is X6.1's line-break census,
/// and the reason is the same: a hand count of this corpus was written
/// down twice, with two different numbers, neither reproducible.
///
/// The population, exactly: every `*.json` fixture; the `?title`
/// literals whose language is German — either by the Fedlex language
/// IRI (`…/DEU`, how a search window carries it) or by an `xml:lang`
/// tag (how a profile carries it); deduplicated by the title string
/// itself, because the rule is about the SHAPE of a title and one act
/// appears in several windows; and of those, the ones whose head is
/// `<one-word type> vom|über|betreffend …`, which is what makes the
/// measurement about the interpolation rather than about German word
/// order (it drops «Verordnung des EDI vom …», «Abkommen zwischen …
/// betreffend …», «Internationaler Pakt vom …» — heads that carry an
/// authority, a counterparty or a two-word type).
#[test]
fn the_promulgation_date_census_of_the_recorded_titles() {
    let mut titles: std::collections::BTreeSet<String> = Default::default();
    for entry in std::fs::read_dir(fixtures_dir()).expect("fixtures dir") {
        let path = entry.expect("dir entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        let Some(bindings) = value["results"]["bindings"].as_array() else {
            continue;
        };
        for binding in bindings {
            let title = &binding["title"];
            if title["type"] != "literal" {
                continue;
            }
            let german = title["xml:lang"] == "de"
                || binding["lang"]["value"]
                    .as_str()
                    .is_some_and(|l| l.ends_with("/DEU"));
            if !german {
                continue;
            }
            if let Some(value) = title["value"].as_str() {
                titles.insert(value.to_string());
            }
        }
    }

    // `<one word> vom|über|betreffend …`, and a subject named with
    // «über» or «betreffend» somewhere in it.
    let head_of = |title: &str| -> Option<&str> {
        let (_, rest) = title.split_once(' ')?;
        ["vom ", "über ", "betreffend "]
            .into_iter()
            .find(|marker| rest.starts_with(marker))
    };
    let population: Vec<&String> = titles
        .iter()
        .filter(|t| t.contains(" über ") || t.contains(" betreffend "))
        .filter(|t| head_of(t).is_some())
        .collect();
    let dated: Vec<&&String> = population
        .iter()
        .filter(|t| head_of(t) == Some("vom "))
        .collect();
    let undated: Vec<&&String> = population
        .iter()
        .filter(|t| head_of(t) != Some("vom "))
        .collect();

    println!(
        "J3.5 census: {} titles, {} dated, {} exceptions",
        population.len(),
        dated.len(),
        undated.len()
    );
    for title in &undated {
        println!("  not dated: {title}");
    }
    assert_eq!(
        (population.len(), dated.len()),
        (77, 73),
        "the figure J3.5 carries is measured here — change the rule in the same commit as the \
         fixtures that move it"
    );
    assert_eq!(undated.len(), 4, "the four the graph leaves dateless");
    assert!(
        undated
            .iter()
            .any(|t| t.contains("Oberaufsicht über die Forstpolizei")),
        "the 1902 act and its predecessor are two of them: {undated:?}"
    );
    assert!(
        undated.iter().any(|t| t.contains("(Entwurf)")),
        "a draft is the third: {undated:?}"
    );
    assert!(
        undated.iter().any(|t| t.contains("Datenschutzgesetz, DSG")),
        "and the act shell without a consolidation is the fourth: {undated:?}"
    );
}

/// X6.1's census, measured by the suite instead of by hand: how many
/// line breaks the recorded manifestations carry, and how many of them
/// sit next to a space and therefore read correctly BY ACCIDENT. The
/// rule's figure comes from this test, so it cannot drift away from the
/// fixtures it describes (BV A″).
#[test]
fn the_line_break_census_of_the_recorded_manifestations() {
    let mut total = 0usize;
    let mut with_space = 0usize;
    let mut without = 0usize;
    let mut files = 0usize;
    for entry in std::fs::read_dir(fixtures_dir()).expect("fixtures dir") {
        let path = entry.expect("dir entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("xml") {
            continue;
        }
        files += 1;
        let xml = std::fs::read_to_string(&path).expect("a recorded manifestation");
        let bytes: Vec<char> = xml.chars().collect();
        let mut at = 0usize;
        while let Some(found) = xml[at..].find("<br") {
            let start = at + found;
            let Some(end) = xml[start..].find('>').map(|e| start + e + 1) else {
                break;
            };
            // «<br», «<br/>», «<br />» — and nothing else that starts so
            let tag = &xml[start..end];
            if tag.trim_end_matches('>').trim_end_matches('/').trim() != "<br" {
                at = end;
                continue;
            }
            total += 1;
            let before = xml[..start].chars().next_back();
            let after = xml[end..].chars().next();
            if before.is_some_and(char::is_whitespace) || after.is_some_and(char::is_whitespace) {
                with_space += 1;
            } else {
                without += 1;
            }
            at = end;
        }
        let _ = &bytes;
    }
    assert_eq!(files, 8, "the recorded manifestations");
    assert_eq!(total, with_space + without);
    println!("BR CENSUS files={files} total={total} with_space={with_space} without={without}");
    // The figure C6.1/X6.1 carries. A recording that changes it must
    // change the rule too — that is what this assertion is for.
    assert_eq!(
        (total, with_space, without),
        (74, 53, 21),
        "the line-break census of the recorded manifestations"
    );
}

/// BV part B′ — the bridge to the LINDAS candidate is COUNTED, not
/// built: 232 of 711 vote titles match the citation shape, and this is
/// what the citation parser makes of three of them today. It reads the
/// last capitalised token as an abbreviation («Landes», «Wahlrechts»,
/// «NFA»), finds no act for it and says so — `kind: unknown`,
/// `act: null`, `unresolved: true`. That is an honest miss, not a wrong
/// answer, and it is the reason the rulebook's C7.4 says «the shape
/// matches», never «the parser resolves it». The grammar that would
/// resolve a dated act title (title + date against JOLux) is a ranked
/// item for the fedlex server, and this test is what it has to change.
#[test]
fn a_dated_act_title_is_not_a_citation_this_parser_resolves() {
    let ctx = fixture_ctx();
    for title in VOTE_TITLES {
        let out = domain::parse_reference(&ctx, title).expect("runs");
        assert!(out.get("error").is_none(), "{title}: {out}");
        assert_eq!(out["total"], 1, "{title}: {out}");
        let reference = &out["references"][0];
        assert_eq!(reference["kind"], "unknown", "{title}: {reference}");
        assert_eq!(
            reference["act"],
            serde_json::Value::Null,
            "no act is found: {reference}"
        );
        assert_eq!(
            reference["unresolved"], true,
            "and the answer says so instead of guessing: {reference}"
        );
        // What it DID read: the last capitalised word, as an
        // abbreviation. That is the gap, named.
        assert!(
            reference["abbreviation"].is_string(),
            "the token it mistook for an abbreviation is in the answer: {reference}"
        );
        assert_eq!(
            reference["eid_candidate"],
            serde_json::Value::Null,
            "no article to address"
        );
    }
    // The three tokens, pinned — a grammar for dated titles must stop
    // producing them.
    let tokens: Vec<String> = VOTE_TITLES
        .iter()
        .map(|title| {
            domain::parse_reference(&ctx, title).expect("runs")["references"][0]["abbreviation"]
                .as_str()
                .unwrap_or_default()
                .to_string()
        })
        .collect();
    assert_eq!(tokens, vec!["Landes", "Wahlrechts", "NFA"], "{tokens:?}");
}

/// Records the three abbreviation pre-queries the vote-title probe
/// needs (BV part B′): a LINDAS vote title is fed to `parse_reference`,
/// which reads its last capitalised token as an abbreviation — the
/// three representatives of the 232 titles that match the citation
/// shape resolve to «Landes», «Wahlrechts» and «NFA»:
/// `cargo test --test e2e record_fixtures_bv_prime_b -- --ignored --nocapture --test-threads 1`
#[test]
#[ignore = "hits the live endpoint once per key; run deliberately"]
fn record_fixtures_bv_prime_b() {
    let ctx = Ctx {
        backend: Backend::recording(FEDLEX_ENDPOINT, fixtures_dir()),
        today: "2026-08-30".into(),
    };
    for title in VOTE_TITLES {
        let out = domain::parse_reference(&ctx, title).expect("record");
        println!(
            "parse_reference «{}…»: kind={} act={} unresolved={}",
            &title[..40.min(title.len())],
            out["references"][0]["kind"],
            out["references"][0]["act"],
            out["references"][0]["unresolved"]
        );
    }
}

/// Three of the 232 LINDAS vote titles that match the citation shape
/// («Bundesbeschluss|Bundesgesetz|Verordnung|Änderung vom <date> …»),
/// read from `testing/lindas-probe/results/c7-fundstelle-titles-all.csv`
/// (measured 2026-08-30): one whose last word is a genitive noun, one
/// with a compound noun in the middle, one carrying an acronym in
/// brackets.
const VOTE_TITLES: [&str; 3] = [
    "Bundesbeschluss vom 26.09.1952 über die Brotgetreideversorgung des Landes",
    "Bundesbeschluss vom 09.10.1970 über die Einführung des Frauenstimm- und Wahlrechts in \
     eidgenössischen Angelegenheiten",
    "Bundesbeschluss vom 03.10.2003 zur Neugestaltung des Finanzausgleichs und der \
     Aufgabenteilung zwischen Bund und Kantonen (NFA)",
];

/// A.1 — the LSV's limit-value table (Anhang 3 Ziff. 2,
/// «Belastungsgrenzwerte») comes out as a header row and data rows;
/// Fedlex marks no <th>, so the first row is the header and the
/// answer says so.
#[test]
fn extract_tables_returns_the_lsv_limit_values_with_a_recognisable_header() {
    let ctx = fixture_ctx();
    let out = domain::extract_tables(&ctx, LSV_VERSION, Some("annex_3/lvl_u1/lvl_2"), None)
        .expect("runs");
    assert_eq!(out["kind"], "norm", "{out}");
    assert_eq!(out["eid"], "annex_3/lvl_u1/lvl_2");
    assert!(out["total"].as_u64().unwrap() >= 1);
    let table = &out["tables"][0];
    assert_eq!(table["header_inferred"], true, "Fedlex marks no <th>");
    let header = table["header"].as_array().expect("header");
    assert!(
        header[0]
            .as_str()
            .unwrap()
            .contains("Empfindlichkeitsstufe"),
        "{header:?}"
    );
    assert!(
        header[1].as_str().unwrap().contains("Planungswert"),
        "{header:?}"
    );
    assert_eq!(table["cols"], 7);
    let data = table["data"].as_array().expect("rows");
    assert!(data.len() >= 4, "{data:?}");
    let stage_i = data
        .iter()
        .find(|r| r[0] == "I")
        .expect("the row of Empfindlichkeitsstufe I");
    assert_eq!(stage_i[1], "50", "Planungswert Tag for stage I");
    assert_eq!(table["rows_total"], data.len());
    assert_eq!(table["truncated"], false);
    assert_eq!(out["provenance"]["served"], "fixture");

    // The whole ordinance: twelve tables, all in annexes; an unknown
    // scope is not-found; an act without tables answers an honest zero.
    let all = domain::extract_tables(&ctx, LSV_VERSION, None, None).expect("runs");
    assert_eq!(all["total"], 12, "{all}");
    assert!(all["tables"]
        .as_array()
        .unwrap()
        .iter()
        .all(|t| t["context_eid"].as_str().unwrap().starts_with("annex_")));
    let none = domain::extract_tables(&ctx, BGOE_VERSION, None, None).expect("runs");
    assert_eq!(none["total"], 0);
    let missing = domain::extract_tables(&ctx, LSV_VERSION, Some("art_999"), None).expect("runs");
    assert_eq!(missing["error"], "not-found");
}

/// A.2 — the citation table: every spelling a marker's or a model's
/// citation may take, taken apart into an act (by abbreviation or SR),
/// an article eId and a path proposal — offline on the recorded
/// abbreviation pre-queries.
#[test]
fn parse_reference_takes_citations_apart_into_readable_addresses() {
    let ctx = fixture_ctx();
    for (text, expected) in reference_table() {
        let out = domain::parse_reference(&ctx, text).expect("runs");
        assert_eq!(out["kind"], "hint", "{text}: {out}");
        assert_eq!(out["total"], 1, "{text}");
        let r = &out["references"][0];
        let e = expected.as_object().unwrap();
        for (field, want) in e {
            match field.as_str() {
                "sr" => assert_eq!(r["act"]["sr"], *want, "{text}: act.sr — {r}"),
                "annex_hint" => assert!(
                    r["annex_hint"]
                        .as_str()
                        .unwrap()
                        .contains(want.as_str().unwrap()),
                    "{text}: annex_hint — {r}"
                ),
                "eid" => assert_eq!(r["eid_candidate"], *want, "{text}: eid_candidate — {r}"),
                _ => assert_eq!(r[field.as_str()], *want, "{text}: {field} — {r}"),
            }
        }
        if e.contains_key("sr") {
            assert_eq!(r["unresolved"], false, "{text}");
            assert!(r["act"]["eli"]
                .as_str()
                .unwrap()
                .starts_with("https://fedlex.data.admin.ch/eli/cc/"));
            assert!(r["act"]["in_force"].is_boolean());
        }
        assert!(
            r["next"].as_str().unwrap().contains("fedlex."),
            "{text}: a next step"
        );
    }
    // «i.V.m.» yields two references, each resolved on its own.
    let joined = domain::parse_reference(&ctx, "Art. 8 EMRK i.V.m. Art. 36 BV").expect("runs");
    assert_eq!(joined["total"], 2, "{joined}");
    assert_eq!(joined["references"][0]["act"]["sr"], "0.101");
    assert_eq!(joined["references"][0]["eid_candidate"], "art_8");
    assert_eq!(joined["references"][1]["act"]["sr"], "101");
    assert_eq!(joined["references"][1]["eid_candidate"], "art_36");
    // A semicolon separates references too; an empty text is refused.
    let two =
        domain::parse_reference(&ctx, "Art. 4 Abs. 2 LSV; Art. 7 Abs. 1 lit. b LSV").expect("runs");
    assert_eq!(two["total"], 2);
    assert_eq!(two["references"][1]["eid_candidate"], "art_7/para_1/lbl_b");
    let empty = domain::parse_reference(&ctx, "   ").expect("runs");
    assert_eq!(empty["error"], "invalid-input");
    // The address the parser proposes is one read_article opens.
    let address = domain::parse_reference(&ctx, "Art. 2 Abs. 2 BGÖ").expect("runs");
    let eid = address["references"][0]["eid_candidate"].as_str().unwrap();
    let read = domain::read_article(&ctx, BGOE_VERSION, eid, None).expect("runs");
    assert_eq!(read["kind"], "norm", "{read}");
    assert!(read["text"].as_str().unwrap().contains("Nationalbank"));
}

/// A.3 — the formal citation graph as two more directions of
/// get_citations; the impact directions stay as they were.
#[test]
fn get_citations_serves_the_formal_citation_graph_as_directions() {
    let ctx = fixture_ctx();
    let cites = domain::get_citations(&ctx, BGOE, "cites").expect("runs");
    assert_eq!(cites["kind"], "norm", "{cites}");
    assert_eq!(cites["direction"], "cites");
    assert!(cites["coverage"]
        .as_str()
        .unwrap()
        .contains("formal citation graph"));
    // J7.1: the coverage sentence names the granularity too — the act,
    // never an article — so no row reads as an article-precise link.
    assert!(
        cites["coverage"]
            .as_str()
            .unwrap()
            .contains("act level only"),
        "the answer says the granularity is the act: {cites}"
    );
    let list = cites["citations"].as_array().unwrap();
    assert_eq!(cites["total"], list.len());
    for c in list {
        assert_eq!(c["from"], BGOE);
        assert!(c["to"]
            .as_str()
            .unwrap()
            .starts_with("https://fedlex.data.admin.ch/eli/"));
    }
    let cited_by = domain::get_citations(&ctx, BGOE, "cited_by").expect("runs");
    assert_eq!(cited_by["direction"], "cited_by");
    assert!(
        cited_by["coverage"]
            .as_str()
            .unwrap()
            .contains("act level only"),
        "{cited_by}"
    );
    assert!(cited_by["citations"]
        .as_array()
        .unwrap()
        .iter()
        .all(|c| c["to"] == BGOE));
    assert_provenance_form(&cites, "2026-08-21");
    let bad = domain::get_citations(&ctx, BGOE, "sideways").expect("runs");
    assert_eq!(bad["error"], "invalid-input");
    assert!(bad["detail"].as_str().unwrap().contains("cites"));
    // BV, rule J7.2: the rulebook found `descriptionFrom` systematically
    // empty; the recording of 2026-08-29 carries it — the description is
    // the citing element's own heading. Pinned so a later emptying is
    // noticed, and so the deduplication is visible: 242 recorded rows,
    // seventeen distinct acts.
    let described: Vec<&serde_json::Value> = list
        .iter()
        .filter(|c| c["description"].as_str().is_some_and(|d| !d.is_empty()))
        .collect();
    assert!(
        !described.is_empty(),
        "descriptionFrom is populated in the recorded answer: {cites}"
    );
    assert!(
        described[0]["description"]
            .as_str()
            .is_some_and(|d| d.starts_with("Art. ")),
        "{:?}",
        described[0]
    );
    // BV, rule J7.4: every version of the act writes its own citation
    // rows — the recorded answer carries 242 of them and the tool
    // reports seventeen distinct acts.
    let targets: std::collections::BTreeSet<&str> =
        list.iter().filter_map(|c| c["to"].as_str()).collect();
    assert_eq!(targets.len(), list.len(), "deduplicated by target: {cites}");
    assert_eq!(list.len(), 17, "{cites}");
    // J7.2: the other direction of the same field — the tool may pass a
    // description through, and must not depend on it. The recorded
    // answer carries targets whose every row has none, and those rows
    // are complete citations all the same.
    let undescribed: Vec<&serde_json::Value> = list
        .iter()
        .filter(|c| !c["description"].as_str().is_some_and(|d| !d.is_empty()))
        .collect();
    assert_eq!(
        undescribed.len(),
        2,
        "two targets carry no description: {cites}"
    );
    assert!(
        undescribed.iter().all(|c| c["from"] == BGOE
            && c["to"]
                .as_str()
                .is_some_and(|t| t.starts_with("https://fedlex.data.admin.ch/eli/"))),
        "a row without a description is still a complete citation row: {cites}"
    );
}

/// A.4 — the BGÖ between 2023-09-01 and 2023-11-01: Art. 17 was
/// re-worded and Art. 23a inserted (AS 2023 584). The diff names both,
/// with the wording before and after.
#[test]
fn compare_versions_finds_the_changed_and_inserted_articles() {
    let ctx = fixture_ctx();
    let out =
        domain::compare_versions(&ctx, BGOE, BGOE_OLDER, BGOE_VERSION, None, None).expect("runs");
    assert_eq!(out["kind"], "norm", "{out}");
    assert_eq!(out["from"]["version"], BGOE_OLDER);
    assert_eq!(out["to"]["version"], BGOE_VERSION);
    assert_eq!(out["from"]["served"], "fixture");
    let added = out["added"].as_array().unwrap();
    assert!(
        added.iter().any(|e| e == "art_23_a"),
        "Art. 23a was inserted: {out}"
    );
    let changed = out["changed"].as_array().unwrap();
    let art_17 = changed
        .iter()
        .find(|c| c["eid"] == "art_17")
        .expect("Art. 17 was re-worded");
    assert!(art_17["heading"].as_str().unwrap().contains("Zugang"));
    let units = art_17["units"].as_array().unwrap();
    let changed_unit = units
        .iter()
        .find(|u| u["change"] == "changed")
        .expect("a paragraph with wording before and after");
    assert_ne!(changed_unit["before"], changed_unit["after"]);
    assert!(changed_unit["before"].as_str().unwrap().len() > 10);
    assert_eq!(changed_unit["before_truncated"], false);
    assert!(
        out["unchanged"].as_u64().unwrap() > 15,
        "most articles unchanged: {out}"
    );
    assert_eq!(out["truncated"], false);
    assert_eq!(out["provenance"]["valid_as_of"], "2023-11-01");

    // Scoped to one element; dates work as version arguments; the
    // same consolidation twice is refused.
    let scoped =
        domain::compare_versions(&ctx, BGOE, "2023-09-01", "20231101", Some("art_17"), None)
            .expect("runs");
    assert_eq!(scoped["compared"], 1, "{scoped}");
    assert_eq!(scoped["changed"][0]["eid"], "art_17");
    let same =
        domain::compare_versions(&ctx, BGOE, BGOE_VERSION, BGOE_VERSION, None, None).expect("runs");
    assert_eq!(same["error"], "invalid-input");
    let unknown =
        domain::compare_versions(&ctx, BGOE, BGOE_OLDER, BGOE_VERSION, Some("art_999"), None)
            .expect("runs");
    assert_eq!(unknown["error"], "not-found");
    // J14.2: the loop resolves the historical version all the same, and
    // then says honestly that this one cannot be read.
    let historical = domain::resolve_consolidation_at(&ctx, KVG, "1997-01-01").expect("runs");
    assert_eq!(
        historical["eli_version"],
        format!("{KVG}/19960101"),
        "the loop resolves the historical version: {historical}"
    );
    let unreadable =
        domain::compare_versions(&ctx, KVG, "19960101", "20260701", None, None).expect("runs");
    assert_eq!(unreadable["error"], "not-found", "{unreadable}");
    assert!(
        unreadable["detail"].as_str().unwrap().contains("PDF-only"),
        "and says honestly why that version cannot be read: {unreadable}"
    );
}

/// A.5 — the node view is a hint that shows edges.
#[test]
fn explore_node_shows_both_directions_capped() {
    let ctx = fixture_ctx();
    let out = domain::explore_node(&ctx, BGOE, Some(20)).expect("runs");
    assert_eq!(out["kind"], "hint", "{out}");
    let outgoing = out["outgoing"].as_array().unwrap();
    assert!(!outgoing.is_empty());
    assert!(outgoing
        .iter()
        .all(|e| e["predicate"].as_str().unwrap().starts_with("http")));
    assert!(out["incoming"].as_array().unwrap().len() <= 20);
    // J0.3: BOTH directions are capped, not the incoming one alone.
    assert!(outgoing.len() <= 20, "both directions are capped: {out}");
    assert!(out["truncated"].is_boolean());
    assert!(out["note"].as_str().unwrap().contains("no interpretation"));
    // J0.3: the answer calls itself a debugging view and points at the
    // typed tools for anything that has to be proven.
    assert!(
        out["note"]
            .as_str()
            .unwrap()
            .contains("a debugging view of the graph")
            && out["note"]
                .as_str()
                .unwrap()
                .contains("use the typed tools to prove anything"),
        "{out}"
    );
    let bad = domain::explore_node(&ctx, "https://evil.example/x", None).expect("runs");
    assert_eq!(bad["error"], "invalid-input");
}

/// A.6 — the BGÖ marks no foreign-language section and no <foreign>
/// island: the honest zero (the mechanics are proven on a synthetic
/// document in the crate's unit tests).
#[test]
fn detect_foreign_content_answers_an_honest_zero_for_the_bgoe() {
    let ctx = fixture_ctx();
    let out = domain::detect_foreign_content(&ctx, BGOE_VERSION, None).expect("runs");
    assert_eq!(out["kind"], "norm", "{out}");
    assert_eq!(out["sections_total"], 0);
    assert_eq!(out["islands_total"], 0);
    assert_eq!(out["truncated"], false);
    assert_eq!(out["provenance"]["served"], "fixture");
}

// --- BR wave 2: the holdings beyond the SR ---------------------------

#[test]
fn treaties_are_found_by_a_title_word_and_profiled() {
    let ctx = fixture_ctx();
    let found =
        domain::find_treaties(&ctx, Some("Menschenrechte"), None, None, Some(10)).expect("runs");
    assert_eq!(found["kind"], "hint", "{found}");
    let hits = found["hits"].as_array().unwrap();
    assert!(!hits.is_empty());
    assert!(hits.len() <= 10);
    // J12.1: the answer says what it counted — the cap it was given,
    // how many it served, and that the window was cut.
    assert_eq!(found["returned"], 10, "{found}");
    assert_eq!(found["limit"], 10, "{found}");
    assert_eq!(
        found["truncated"], true,
        "39 recorded processes, ten served — the answer says what it counted: {found}"
    );
    let process = hits[0]["process"].as_str().unwrap().to_string();
    assert!(process.contains("/eli/treaty/"), "{process}");
    assert!(hits[0]["title"]
        .as_str()
        .unwrap()
        .to_lowercase()
        .contains("menschenrechte"));
    let info = domain::get_treaty_info(&ctx, &process, Some("de")).expect("runs");
    assert_eq!(info["kind"], "norm", "{info}");
    assert_eq!(info["treaty"]["process"], process);
    assert!(info["treaty"]["party_countries"].is_array());
    let none = domain::find_treaties(&ctx, None, None, None, None).expect("runs");
    assert_eq!(none["error"], "invalid-input");
}

#[test]
fn the_genesis_of_the_ndsg_is_reachable_drafts_consultations_documents() {
    let ctx = fixture_ctx();
    let drafts = domain::get_drafts(&ctx, NDSG).expect("runs");
    assert_eq!(drafts["kind"], "norm", "{drafts}");
    let list = drafts["drafts"].as_array().unwrap();
    assert!(!list.is_empty(), "the nDSG came from a draft: {drafts}");
    // J11.1: the row is a draft IRI, and the answer says what a draft
    // IRI is good for — the entry into the genesis.
    assert!(
        list[0]["draft"].as_str().unwrap().contains("/eli/dl/proj/"),
        "a draft IRI, not an act: {drafts}"
    );
    assert!(
        drafts["note"]
            .as_str()
            .is_some_and(|n| n.contains("the entry to fedlex.get_consultations")),
        "the answer says it is the entry into the genesis: {drafts}"
    );
    // J11.2: the federated key travels with the row — the Curia Vista
    // business number, named as such in the note.
    assert_eq!(
        list[0]["parliament_draft_id"], "17.059",
        "the Curia Vista number is carried through: {drafts}"
    );
    assert!(
        drafts["note"]
            .as_str()
            .is_some_and(|n| n.contains("Curia Vista")),
        "{drafts}"
    );
    let consultations =
        domain::get_consultations(&ctx, Some(NDSG), None, None, None).expect("runs");
    assert_eq!(consultations["kind"], "hint", "{consultations}");
    assert!(!consultations["drafts_considered"]
        .as_array()
        .unwrap()
        .is_empty());
    // J18.3: each layer points at the next — the consultations are
    // walked from the very draft get_drafts named.
    assert_eq!(
        consultations["drafts_considered"][0], list[0]["draft"],
        "the consultations are walked from the draft get_drafts named"
    );
    let cons = consultations["consultations"].as_array().unwrap();
    assert_eq!(consultations["total"], cons.len());
    let first = cons.first().expect("the recorded consultation");
    let docs = domain::get_consultation_documents(&ctx, first["consultation"].as_str().unwrap())
        .expect("runs");
    assert_eq!(docs["kind"], "norm", "{docs}");
    assert!(docs["documents"].is_array());
    assert_eq!(
        first["draft"], list[0]["draft"],
        "the consultation points back at its draft"
    );
    assert_eq!(
        docs["documents"].as_array().unwrap().len(),
        14,
        "the layer the consultation points at: {docs}"
    );
    assert_eq!(docs["consultation"], first["consultation"]);
    let fga = domain::get_fga_documents(&ctx, NDSG).expect("runs");
    assert_eq!(fga["kind"], "norm", "{fga}");
    assert!(fga["documents"].is_array());
    // J9.1: the answer says what these documents are — materials for
    // interpretation, never law in force.
    assert!(
        fga["note"].as_str().is_some_and(
            |n| n.contains("materials for interpretation") && n.contains("not law in force")
        ),
        "{fga}"
    );
    let neither = domain::get_consultations(&ctx, None, None, None, None).expect("runs");
    assert_eq!(neither["error"], "invalid-input");
    // The EnG: a draft with Federal Gazette documents on record — the
    // non-empty side of the genesis; its consultations answer an
    // honest empty list on BOTH paths the widened query walks (BS,
    // recorded) — the nDSG is the covered example of the closed gap.
    let eng_fga = domain::get_fga_documents(&ctx, ENG).expect("runs");
    let docs = eng_fga["documents"].as_array().unwrap();
    assert!(!docs.is_empty(), "{eng_fga}");
    assert!(docs[0]["document"].as_str().unwrap().contains("/eli/fga/"));
    let eng_drafts = domain::get_drafts(&ctx, ENG).expect("runs");
    assert!(!eng_drafts["drafts"].as_array().unwrap().is_empty());
    assert_eq!(
        eng_drafts["drafts"][0]["parliament_draft_id"], "13.074",
        "{eng_drafts}"
    );
    let eng_cons = domain::get_consultations(&ctx, Some(ENG), None, None, None).expect("runs");
    assert_eq!(
        eng_cons["total"], 0,
        "recorded: none on either path — {eng_cons}"
    );
    assert!(eng_cons["note"]
        .as_str()
        .is_some_and(|n| n.contains("either path")));
}

#[test]
fn the_official_compilation_chain_of_the_bgoe() {
    let ctx = fixture_ctx();
    let oc = domain::get_oc_act(&ctx, BGOE).expect("runs");
    assert_eq!(oc["kind"], "norm", "{oc}");
    let oc_eli = oc["oc"].as_str().expect("oc eli").to_string();
    assert!(oc_eli.contains("/eli/oc/"), "{oc_eli}");
    // J19.1: the link is READ from the graph's basicAct row, not
    // rewritten from the act's own numbers — the dates and the memorial
    // beside it are what a string rewrite could never invent.
    assert_eq!(
        oc["oc"], "https://fedlex.data.admin.ch/eli/oc/2006/355",
        "{oc}"
    );
    assert_eq!(
        oc["publication_date"], "2006-06-20",
        "the graph's row, not a rewritten string: {oc}"
    );
    assert_eq!(
        oc["memorial"], "https://fedlex.data.admin.ch/eli/collection/oc/2006/24",
        "{oc}"
    );
    assert_provenance_form(&oc, "2026-08-21");
    // J8.2: the official-compilation level answers metadata only — the
    // consolidated wording is read from the classified compilation, and
    // the wording tools refuse an oc ELI outright.
    assert!(
        oc.get("text").is_none() && oc.get("documents").is_none(),
        "get_oc_act answers metadata only — no OC text: {oc}"
    );
    let no_oc_text = domain::get_structure(&ctx, &oc_eli, None, None).expect("runs");
    assert_eq!(
        no_oc_text["error"], "invalid-input",
        "the wording tools take a consolidation version, never an oc ELI: {no_oc_text}"
    );
    // J8.3 and J1.2: genre and responsible office are carried here —
    // the two fields the consolidated profile leaves out.
    assert_eq!(oc["genre_label"], "Grunderlass", "{oc}");
    assert!(
        oc["genre"]
            .as_str()
            .is_some_and(|g| g.contains("/vocabulary/legal-resource-genre/")),
        "{oc}"
    );
    assert!(
        oc["responsible_office"]
            .as_str()
            .is_some_and(|o| o.contains("/vocabulary/legal-institution/")),
        "the field the consolidated level leaves empty: {oc}"
    );
    let memorial = domain::get_memorial(&ctx, &oc_eli, Some(20)).expect("runs");
    assert_eq!(memorial["kind"], "norm", "{memorial}");
    assert!(memorial["memorial"]
        .as_str()
        .unwrap()
        .contains("/eli/collection/"));
    assert!(!memorial["acts"].as_array().unwrap().is_empty());
    // J8.5: the answer names its cap and what it served under it.
    assert_eq!(
        memorial["limit"], 20,
        "the answer names its cap: {memorial}"
    );
    assert_eq!(memorial["returned"], 15, "the recorded issue: {memorial}");
    assert_eq!(
        memorial["truncated"], false,
        "fifteen acts fit under the cap: {memorial}"
    );
    // J19.3: the address is the issue's own ELI — never an AS page
    // reference, which is refused before any query.
    assert_eq!(
        memorial["memorial"], "https://fedlex.data.admin.ch/eli/collection/oc/2006/24",
        "{memorial}"
    );
    let page_ref = domain::get_memorial(&ctx, "AS 2006 2319", None).expect("runs");
    assert_eq!(
        page_ref["error"], "invalid-input",
        "the issue ELI, never a page reference: {page_ref}"
    );
    // The two tools refuse each other's input with a pointer.
    let wrong = domain::get_oc_act(&ctx, &oc_eli).expect("runs");
    assert_eq!(wrong["error"], "invalid-input");
    assert!(
        wrong["detail"]
            .as_str()
            .unwrap()
            .contains("consolidation ELI"),
        "{wrong}"
    );
    let wrong = domain::get_memorial(&ctx, BGOE, None).expect("runs");
    assert_eq!(wrong["error"], "invalid-input");
    assert!(wrong["detail"].as_str().unwrap().contains("get_oc_act"));
    let unknown = domain::get_oc_act(&ctx, UNKNOWN).expect("runs");
    assert_eq!(unknown["error"], "not-found");
}

// --- stage-one discipline ----------------------------------------------

// --- BS: the polite brake, proven on a frozen clock; the consultation gap closed --

/// The 2016/17 consultation of the DSG revision — reached from the
/// nDSG's parliamentary draft through its legislative task (the shape
/// the live probe at BS found; recorded).
const DSG_REVISION_CONSULTATION: &str = "https://fedlex.data.admin.ch/eli/dl/proj/6016/61/cons_1";
const NDSG_DRAFT: &str = "https://fedlex.data.admin.ch/eli/dl/proj/8022/0491";

/// A counting context whose brake runs on a frozen clock: every wait
/// is recorded instead of slept, every live request counted. Calls in
/// a row model requests arriving at the SAME instant (the clock does
/// not move while «sleeping»); `clock.advance` lets time pass.
fn braked_ctx(
    rate: f64,
    burst: f64,
    max_wait_s: u64,
) -> (
    Ctx,
    std::sync::Arc<FrozenClock>,
    std::sync::Arc<std::sync::atomic::AtomicUsize>,
    std::sync::Arc<std::sync::atomic::AtomicUsize>,
) {
    let clock = FrozenClock::new();
    let throttle = UpstreamThrottle::frozen(
        rate,
        burst,
        std::time::Duration::from_secs(max_wait_s),
        clock.clone(),
    );
    let (backend, selects, fetches) = Backend::counting_throttled(
        fixtures_dir(),
        ManifestationCache::default_sized(),
        vec!["2026-08-29T10:00:00Z".into()],
        throttle,
    );
    let ctx = Ctx {
        backend,
        today: "2026-08-29".into(),
    };
    (ctx, clock, selects, fetches)
}

fn recorded_sleeps_ms(clock: &FrozenClock) -> Vec<u128> {
    clock
        .sleeps()
        .iter()
        .map(std::time::Duration::as_millis)
        .collect()
}

/// Six tool calls within one frozen second, each one live request, at
/// 2/s with a burst of 4: four go at once, the fifth and sixth wait
/// 500 and 1000 ms — measured as the sleeps the brake asked the frozen
/// clock for, with all six answers intact.
#[test]
fn the_polite_brake_admits_the_burst_and_queues_the_rest() {
    use std::sync::atomic::Ordering;
    let (ctx, clock, selects, _) = braked_ctx(2.0, 4.0, 5);
    for _ in 0..6 {
        let out = domain::get_law_metadata(&ctx, KVG, None).expect("runs");
        assert!(out.get("error").is_none(), "{out}");
        assert_eq!(out["kind"], "norm");
    }
    assert_eq!(selects.load(Ordering::SeqCst), 6, "six live requests");
    assert_eq!(
        recorded_sleeps_ms(&clock),
        vec![500, 1000],
        "four immediate, two queued"
    );
    let brake = ctx
        .backend
        .throttle()
        .expect("the counting double is braked");
    assert_eq!(brake.admitted(), 6);
    assert_eq!(brake.refused(), 0);
}

/// Twenty calls at once against 2/s, burst 4, five seconds of
/// patience: fourteen are admitted in arrival order (the burst, then
/// ten reservations up to the limit), the six beyond it come back as
/// the typed `upstream-busy` — every one naming 5500 ms, the wait a
/// free token is away — and none of them reaches the endpoint. The
/// same refusal arrives through the vendored bridge, and time passing
/// on the clock admits again.
#[test]
fn beyond_five_seconds_of_waiting_the_brake_answers_upstream_busy() {
    use std::sync::atomic::Ordering;
    let (ctx, clock, selects, _) = braked_ctx(2.0, 4.0, 5);
    let mut admitted = 0;
    let mut refusals = Vec::new();
    for i in 0..20 {
        let out = domain::get_law_metadata(&ctx, KVG, None).expect("runs");
        if out["error"] == "upstream-busy" {
            refusals.push((i, out));
        } else {
            assert!(out.get("error").is_none(), "{out}");
            assert!(
                refusals.is_empty(),
                "admissions come first, in arrival order — call {i} after a refusal"
            );
            admitted += 1;
        }
    }
    assert_eq!(admitted, 14, "the burst plus ten reservations");
    assert_eq!(refusals.len(), 6);
    for (_, out) in &refusals {
        assert_eq!(out["retry_after_ms"], 5500, "{out}");
        assert!(
            out["detail"]
                .as_str()
                .is_some_and(|d| d.contains("polite brake") && d.contains("2 live requests/s")),
            "{out}"
        );
        assert_eq!(
            out.as_object().unwrap().len(),
            3,
            "error, detail, retry_after_ms: {out}"
        );
    }
    assert_eq!(
        selects.load(Ordering::SeqCst),
        14,
        "a refused call never reaches the endpoint"
    );
    assert_eq!(
        recorded_sleeps_ms(&clock),
        (1..=10).map(|n| n * 500).collect::<Vec<u128>>()
    );
    // The vendored bridge path types the same refusal.
    let bridged = domain::get_drafts(&ctx, NDSG).expect("runs");
    assert_eq!(bridged["error"], "upstream-busy", "{bridged}");
    assert_eq!(bridged["retry_after_ms"], 5500);
    // Two seconds later four tokens refilled against ten reserved: a
    // deficit of seven, a wait of 3.5 s — admitted.
    clock.advance(std::time::Duration::from_secs(2));
    let again = domain::get_law_metadata(&ctx, KVG, None).expect("runs");
    assert!(again.get("error").is_none(), "{again}");
    assert_eq!(recorded_sleeps_ms(&clock).last(), Some(&3500));
}

/// Cache hits and fixtures never take a token: three reads of one
/// cached version cost the brake nothing after the first, and the
/// fixtures backend has no brake at all.
#[test]
fn cache_hits_and_fixtures_never_touch_the_brake() {
    use std::sync::atomic::Ordering;
    let (ctx, clock, selects, fetches) = braked_ctx(2.0, 4.0, 5);
    let first = domain::read_article(&ctx, BGOE_VERSION, "art_6", Some("de")).expect("runs");
    assert_eq!(first["provenance"]["served"], "live");
    let brake = ctx.backend.throttle().expect("braked");
    assert_eq!(brake.admitted(), 2, "one resolution query, one fetch");
    for eid in ["art_17", "art_6", "art_1"] {
        let out = domain::read_article(&ctx, BGOE_VERSION, eid, Some("de")).expect("runs");
        assert_eq!(out["provenance"]["served"], "cache", "{out}");
    }
    assert_eq!(brake.admitted(), 2, "three cache hits took no token");
    assert_eq!(selects.load(Ordering::SeqCst), 1);
    assert_eq!(fetches.load(Ordering::SeqCst), 1);
    assert!(clock.sleeps().is_empty());

    let fixtures = fixture_ctx();
    assert!(
        fixtures.backend.throttle().is_none(),
        "fixtures are never braked"
    );
    for _ in 0..30 {
        let out = domain::get_law_metadata(&fixtures, KVG, None).expect("runs");
        assert!(out.get("error").is_none(), "{out}");
    }
}

/// The consultation gap of BR, closed with the graph's real shape: the
/// nDSG's parliamentary draft reaches the 2016/17 consultation of the
/// DSG revision through a legislative task — with its German title,
/// the dates from the opening phase, the status — and the documents
/// under the consultation's sub-tasks are readable: the Vorlage, the
/// explanatory report, cover letters, the address list, the result
/// report.
/// BT′ audit: the «from»-free rule is a property of every query the
/// consultation path sends — the two hand-written ones AND the vendored
/// primitives' (the draft query, the opinion query). The counting double
/// keeps every query it was asked to run, so the guard reads the wire
/// rather than the source.
#[test]
fn every_query_of_the_consultation_path_stays_from_free() {
    let (backend, _, _) = Backend::counting(
        fixtures_dir(),
        ManifestationCache::default_sized(),
        vec!["2026-08-29T10:00:00Z".into()],
    );
    let ctx = Ctx {
        backend,
        today: "2026-08-29".into(),
    };
    let drafts = domain::get_drafts(&ctx, NDSG).expect("runs");
    assert!(!drafts["drafts"].as_array().unwrap().is_empty());
    let consultations =
        domain::get_consultations(&ctx, Some(NDSG), None, None, None).expect("runs");
    assert!(!consultations["consultations"]
        .as_array()
        .unwrap()
        .is_empty());
    let documents =
        domain::get_consultation_documents(&ctx, DSG_REVISION_CONSULTATION).expect("runs");
    assert!(!documents["documents"].as_array().unwrap().is_empty());
    let queries = ctx.backend.seen_queries();
    assert!(
        queries.len() >= 4,
        "the path sends the draft query, the consultation query and both document queries: {}",
        queries.len()
    );
    for query in &queries {
        let has_from = query
            .split(|c: char| !c.is_alphanumeric())
            .any(|word| word.eq_ignore_ascii_case("from"));
        assert!(
            !has_from,
            "the federal WAF blocks a long query carrying the word «from»: {query}"
        );
    }
}

#[test]
fn the_dsg_revision_consultation_is_reachable_with_dates_and_documents() {
    let ctx = fixture_ctx();
    let out = domain::get_consultations(&ctx, Some(NDSG), None, None, None).expect("runs");
    assert_eq!(out["kind"], "hint", "{out}");
    // J10.4: the stage-one line points the caller away from law in
    // force — the genesis is what this path answers.
    let tools = FedlexServer::tool_router().list_all();
    let line = tools
        .iter()
        .find(|t| t.name == "fedlex.get_consultations")
        .and_then(|t| t.description.as_deref())
        .expect("the stage-one line");
    assert!(
        line.contains("never for law in force"),
        "the line points away from the acts in force: {line}"
    );
    assert_eq!(out["drafts_considered"][0], NDSG_DRAFT);
    let cons = out["consultations"].as_array().unwrap();
    assert!(
        !cons.is_empty(),
        "the widened path finds the consultation: {out}"
    );
    let first = &cons[0];
    assert_eq!(first["consultation"], DSG_REVISION_CONSULTATION);
    assert_eq!(first["draft"], NDSG_DRAFT);
    assert_eq!(first["start_date"], "2016-12-21");
    assert_eq!(first["end_date"], "2017-04-04");
    assert!(
        first["status"]
            .as_str()
            .is_some_and(|s| s.contains("/vocabulary/consultation-status/")),
        "{first}"
    );
    assert!(
        first["title"]
            .as_str()
            .is_some_and(|t| t.contains("Totalrevision des Datenschutzgesetzes")),
        "{first}"
    );
    assert!(first["institution"]
        .as_str()
        .is_some_and(|i| i.contains("legal-institution")));
    // The status filter, by last segment and by a segment nobody has.
    let last_segment = first["status"]
        .as_str()
        .unwrap()
        .rsplit('/')
        .next()
        .unwrap();
    let filtered =
        domain::get_consultations(&ctx, Some(NDSG), None, Some(last_segment), None).expect("runs");
    assert_eq!(filtered["total"], 1, "{filtered}");
    let none =
        domain::get_consultations(&ctx, Some(NDSG), None, Some("no-such"), None).expect("runs");
    assert_eq!(none["total"], 0);
    // By draft IRI directly: the same consultation.
    let by_draft =
        domain::get_consultations(&ctx, None, Some(NDSG_DRAFT), None, None).expect("runs");
    assert_eq!(
        by_draft["consultations"][0]["consultation"],
        DSG_REVISION_CONSULTATION
    );

    let docs = domain::get_consultation_documents(&ctx, DSG_REVISION_CONSULTATION).expect("runs");
    assert_eq!(docs["kind"], "norm", "{docs}");
    let list = docs["documents"].as_array().unwrap();
    assert_eq!(
        list.len(),
        14,
        "the frozen fixture carries fourteen: {docs}"
    );
    assert_eq!(docs["total"], list.len());
    assert!(
        list.iter()
            .any(|d| d["role"] == "draft" && d["title"] == "Vorlage"),
        "the text put into consultation: {docs}"
    );
    assert!(
        list.iter()
            .any(|d| d["role"] == "related" && d["title"] == "Bericht"),
        "the explanatory report: {docs}"
    );
    assert!(list.iter().all(|d| d["document"]
        .as_str()
        .unwrap()
        .starts_with(DSG_REVISION_CONSULTATION)));
    assert!(list.iter().all(|d| {
        matches!(
            d["kind"].as_str(),
            Some("DraftDocument" | "DraftRelatedDocument")
        ) || d["role"] == "opinion"
    }));
    assert_eq!(docs["truncated"], false);
    // J20.4: both entry points are validated by shape before any query
    // runs — a foreign host and a draft that is no IRI are refused
    // without the backend being touched.
    let foreign =
        domain::get_consultation_documents(&ctx, "https://evil.example/eli/dl/proj/2016/1")
            .expect("runs");
    assert_eq!(
        foreign["error"], "invalid-input",
        "the shape gate refuses before any query: {foreign}"
    );
    let bad_draft =
        domain::get_consultations(&ctx, None, Some("not-an-iri"), None, None).expect("runs");
    assert_eq!(bad_draft["error"], "invalid-input", "{bad_draft}");
}

/// BS: the consultation gap — re-records the nDSG and EnG consultation
/// keys with the widened query and records the DSG-revision
/// consultation's documents (one polite request per key):
/// `cargo test --test e2e record_fixtures_bs -- --ignored --nocapture --test-threads 1`
#[test]
#[ignore = "hits the live endpoint once per key; run deliberately"]
fn record_fixtures_bs() {
    let ctx = Ctx {
        backend: Backend::recording(FEDLEX_ENDPOINT, fixtures_dir()),
        today: "2026-08-21".into(),
    };
    for act in [NDSG, ENG] {
        let out = domain::get_consultations(&ctx, Some(act), None, None, None).expect("record");
        println!("get_consultations {act}: {}", out["total"]);
        if let Some(c) = out["consultations"][0]["consultation"].as_str() {
            let docs = domain::get_consultation_documents(&ctx, c).expect("record");
            println!(
                "get_consultation_documents {c}: {} (under tasks {}, opinions {})",
                docs["total"], docs["under_tasks"], docs["opinions"]
            );
        }
    }
}

// --- BT: the citation pair — check_quote and cite -----------------------

/// A sentence of Art. 6 Abs. 1 BGÖ (the version recorded at BQ) — the
/// text the quote tests quote from, as read_article serves it.
const BGOE_ART_6_1: &str = "Jede Person hat das Recht, amtliche Dokumente einzusehen und von den \
                            Behörden Auskünfte über den Inhalt amtlicher Dokumente zu erhalten.";

/// The quote table: (element, quote) → verified, and which segments
/// were found. Every row is checked against the recorded manifestations
/// — nothing is fetched, and nothing judges whether the sentence is true.
fn quote_table() -> Vec<(&'static str, &'static str, &'static str, bool, Vec<bool>)> {
    vec![
        // exact
        (BGOE_VERSION, "art_6/para_1", "Jede Person hat das Recht, amtliche Dokumente einzusehen", true, vec![true]),
        // the whole paragraph, exact
        (BGOE_VERSION, "art_6/para_1", BGOE_ART_6_1, true, vec![true]),
        // line breaks and doubled spaces inside the quote
        (BGOE_VERSION, "art_6/para_1", "Jede Person hat das Recht,\n  amtliche   Dokumente\neinzusehen", true, vec![true]),
        // an omission mark: two segments, in order
        (BGOE_VERSION, "art_6/para_1", "Jede Person hat das Recht … zu erhalten.", true, vec![true, true]),
        // three segments, three spellings of the omission
        (BGOE_VERSION, "art_6/para_1", "amtliche Dokumente [...] Auskünfte ... zu erhalten", true, vec![true, true, true]),
        // a segment that is not there (it is in Abs. 2)
        (BGOE_VERSION, "art_6/para_1", "Jede Person hat das Recht … Kopien davon angefordert", false, vec![true, false]),
        // right words, wrong order — the second segment must follow the first
        (BGOE_VERSION, "art_6/para_1", "zu erhalten … Jede Person", false, vec![true, false]),
        // the quote of Art. 6 Abs. 1 held against Art. 7: false, not an error
        (BGOE_VERSION, "art_7", "Jede Person hat das Recht, amtliche Dokumente einzusehen", false, vec![false]),
        // against the whole article: the paragraph's sentence is in it
        (BGOE_VERSION, "art_6", "Jede Person hat das Recht, amtliche Dokumente einzusehen", true, vec![true]),
        // the heading IS part of the article's text as read_article serves it (BT′)
        (BGOE_VERSION, "art_6", "Öffentlichkeitsprinzip", true, vec![true]),
        // … but not of a paragraph's text
        (BGOE_VERSION, "art_6/para_1", "Öffentlichkeitsprinzip", false, vec![false]),
        // an annex level: number and title of the level, then the table's first cell (the
        // manifestation's line breaks become one space)
        (LSV_VERSION, "annex_3/lvl_u1/lvl_2", "2 Belastungsgrenzwerte Empfindlichkeitsstufe (Art. 43)", true, vec![true]),
        // a list item: the letter is part of its text
        (LSV_VERSION, "art_7/para_1/lbl_b", "b. dass die von der Anlage allein erzeugten Lärmimmissionen die Planungswerte nicht überschreiten.", true, vec![true]),
        // typographic quotation marks: the KVG writes « », the quote writes " and „ “
        (KVG_VERSION_PLACEHOLDER, "art_95_a/para_4", "Die Ausdrücke \"Mitgliedstaaten der Europäischen Union\", „Mitgliedstaaten der Europäischen Gemeinschaft“", true, vec![true]),
        // a dash: the LSV writes «7–9» (en dash), the quote a hyphen
        (LSV_VERSION, "art_45/para_3/listintro", "Emissionsbegrenzungen (Art. 4, 7-9 und 12)", true, vec![true]),
        // X18.5: the LSV writes «37<i>a</i>» — the quote runs straight through the inline override
        (LSV_VERSION, "art_45/para_3/listintro", "Ermittlung und Beurteilung von Lärmimmissionen (Art. 36, 37, 37a und 40)", true, vec![true]),
        // a no-break space in the LSV's text, a plain one in the quote
        (LSV_VERSION, "art_1/para_1", "Diese Verordnung soll vor schädlichem und lästigem Lärm", true, vec![true]),
        // case is wording: false
        (BGOE_VERSION, "art_6/para_1", "jede person hat das recht", false, vec![false]),
        // longer than the text: false, the missing tail says why
        (BGOE_VERSION, "art_6/para_1", "Jede Person hat das Recht, amtliche Dokumente einzusehen und von den Behörden Auskünfte über den Inhalt amtlicher Dokumente zu erhalten, und noch einen Satz, den der Absatz nicht hat", false, vec![false]),
    ]
}

const KVG_VERSION_PLACEHOLDER: &str = "<kvg>";

/// Every row of the quote table answers exactly as the table says —
/// verified true or false with the segments' findings, offsets in
/// order, never an error.
#[test]
fn check_quote_answers_the_quote_table_verbatim_and_normalised() {
    let ctx = fixture_ctx();
    let kvg_version = recorded_version_for(KVG).expect("a recorded KVG manifestation");
    for (version, eid, quote, verified, found) in quote_table() {
        let version = if version == KVG_VERSION_PLACEHOLDER {
            kvg_version.as_str()
        } else {
            version
        };
        let out = domain::check_quote(&ctx, version, eid, quote, Some("de")).expect("runs");
        assert!(out.get("error").is_none(), "{quote}: {out}");
        assert_eq!(out["kind"], "norm");
        assert_eq!(out["verified"], verified, "{quote}: {out}");
        let segments = out["segments"].as_array().unwrap();
        let findings: Vec<bool> = segments.iter().map(|s| s["found"] == true).collect();
        assert_eq!(findings, found, "{quote}: {out}");
        if quote == "Öffentlichkeitsprinzip" && eid == "art_6" {
            assert_eq!(
                segments[0]["at"], 7,
                "«Art. 6 » precedes the heading in the text as served: {out}"
            );
        }
        let mut last_at = None;
        for segment in segments {
            if segment["found"] == true {
                let at = segment["at"].as_u64().expect("an offset");
                assert!(last_at.is_none_or(|prev| at > prev), "in order: {out}");
                last_at = Some(at);
                assert!(segment["text"].as_str().unwrap().len() <= quote.len());
            } else {
                assert!(segment.get("at").is_none());
            }
        }
        assert!(out["text_length"].as_u64().unwrap() > 0);
        assert_eq!(out["provenance"]["served"], "fixture");
        assert!(out["note"]
            .as_str()
            .unwrap()
            .contains("never that a statement is true"));
        assert!(out["note"].as_str().unwrap().contains(
            "the article's number and heading, the paragraph numbers and the list letters \
             included, footnotes excluded"
        ));
        assert!(out["note"]
            .as_str()
            .unwrap()
            .contains("carries no «Anhang n»"));
    }
}

/// The refusals of check_quote: an empty quote and a quote of only
/// omission marks are invalid-input; an eId the version does not
/// carry is not-found; a bad version is refused before any read.
#[test]
fn check_quote_refuses_what_is_not_a_quote_and_names_a_missing_element() {
    let ctx = fixture_ctx();
    let empty = domain::check_quote(&ctx, BGOE_VERSION, "art_6", "   ", None).expect("runs");
    assert_eq!(empty["error"], "invalid-input", "{empty}");
    let marks = domain::check_quote(&ctx, BGOE_VERSION, "art_6", "… [...]", None).expect("runs");
    assert_eq!(marks["error"], "invalid-input", "{marks}");
    let missing =
        domain::check_quote(&ctx, BGOE_VERSION, "art_999", "Jede Person", None).expect("runs");
    assert_eq!(missing["error"], "not-found", "{missing}");
    assert!(missing["subject"].as_str().unwrap().ends_with("#art_999"));
    let bad =
        domain::check_quote(&ctx, "https://example.org/x", "art_6", "Jede", None).expect("runs");
    assert_eq!(bad["error"], "invalid-input", "{bad}");
    let long = "x".repeat(20_001);
    let too_long = domain::check_quote(&ctx, BGOE_VERSION, "art_6", &long, None).expect("runs");
    assert_eq!(too_long["error"], "invalid-input");
}

/// The canonical labels: article, paragraph, letter, suffixed article,
/// an article's single paragraph, an annex — with the abbreviation of
/// the language read and the SR number from the taxonomy.
#[test]
fn cite_names_the_canonical_fundstelle_in_the_language_read() {
    let ctx = fixture_ctx();
    let kvg_version = recorded_version_for(KVG).expect("a recorded KVG manifestation");
    for (version, eid, lang, label, sr) in [
        (BGOE_VERSION, "art_6", "de", "Art. 6 BGÖ", "152.3"),
        (
            BGOE_VERSION,
            "art_6/para_1",
            "de",
            "Art. 6 Abs. 1 BGÖ",
            "152.3",
        ),
        (
            LSV_VERSION,
            "art_7/para_1/lbl_b",
            "de",
            "Art. 7 Abs. 1 Bst. b LSV",
            "814.41",
        ),
        (BGOE_VERSION, "art_23_a", "de", "Art. 23a BGÖ", "152.3"),
        (BGOE_VERSION, "art_23_a/para", "de", "Art. 23a BGÖ", "152.3"),
        (
            kvg_version.as_str(),
            "art_25_a/para_1/lbl_b",
            "de",
            "Art. 25a Abs. 1 Bst. b KVG",
            "832.10",
        ),
        (
            kvg_version.as_str(),
            "art_25/para_2/lbl_f_bis",
            "de",
            "Art. 25 Abs. 2 Bst. fbis KVG",
            "832.10",
        ),
        (BGOE_VERSION, "annex_u1", "de", "Anhang BGÖ", "152.3"),
        (LSV_VERSION, "annex_3", "de", "Anhang 3 LSV", "814.41"),
        (
            LSV_VERSION,
            "annex_3/lvl_u1",
            "de",
            "Anhang 3 LSV",
            "814.41",
        ),
        (
            LSV_VERSION,
            "annex_3/lvl_u1/lvl_2",
            "de",
            "Anhang 3 LSV",
            "814.41",
        ),
        (BGOE_VERSION, "art_6", "fr", "art. 6 LTrans", "152.3"),
        (
            BGOE_VERSION,
            "art_6/para_1",
            "it",
            "art. 6 cpv. 1 LTras",
            "152.3",
        ),
    ] {
        let out = domain::cite(&ctx, version, eid, Some(lang)).expect("runs");
        assert!(out.get("error").is_none(), "{eid} {lang}: {out}");
        assert_eq!(out["label"], label, "{out}");
        assert_eq!(out["sr"], sr, "{out}");
        assert_eq!(out["kind"], "norm");
        assert_eq!(out["lang"], lang);
        assert_eq!(out["eli_version"], version);
        assert!(out["valid_as_of"].as_str().unwrap().len() == 10);
        assert!(
            out["title"][lang].as_str().is_some_and(|t| !t.is_empty()),
            "{out}"
        );
        assert_eq!(out["provenance"]["served"], "fixture");
    }
    let annex = domain::cite(&ctx, LSV_VERSION, "annex_3/lvl_u1", None).expect("runs");
    assert_eq!(annex["annex"], "Anhang 3");
    assert!(annex["article"].is_null());
    let letter = domain::cite(&ctx, LSV_VERSION, "art_7/para_1/lbl_b", None).expect("runs");
    assert_eq!(letter["article"], "7");
    assert_eq!(letter["paragraph"], "1");
    assert_eq!(letter["letter"], "b");
    assert_eq!(letter["short"], "LSV");
    assert!(letter["title"]["de"]
        .as_str()
        .unwrap()
        .contains("Lärmschutz"));
    // An annex WRAPPER is not an element the manifestation addresses:
    // both tools resolve it to the first level and say which eId they
    // read; an unnumbered annex is «Anhang» alone with annex: null.
    let unnumbered = domain::cite(&ctx, BGOE_VERSION, "annex_u1", None).expect("runs");
    assert!(unnumbered["annex"].is_null(), "{unnumbered}");
    assert_eq!(unnumbered["eid"], "annex_u1/lvl_u1", "{unnumbered}");
    assert!(unnumbered["note"].as_str().unwrap().contains("annex: null"));
    let wrapper = domain::cite(&ctx, LSV_VERSION, "annex_3", None).expect("runs");
    assert_eq!(wrapper["label"], "Anhang 3 LSV");
    assert_eq!(wrapper["eid"], "annex_3/lvl_u1", "{wrapper}");
    let wrapper_quote =
        domain::check_quote(&ctx, LSV_VERSION, "annex_3", "Belastungsgrenzwerte", None)
            .expect("runs");
    assert_eq!(wrapper_quote["verified"], true, "{wrapper_quote}");
    assert_eq!(wrapper_quote["eid"], "annex_3/lvl_u1");
    assert_eq!(wrapper["designation"], "annex", "{wrapper}");
    assert_eq!(unnumbered["designation"], "annex", "{unnumbered}");
    let no_such_annex = domain::cite(&ctx, LSV_VERSION, "annex_99", None).expect("runs");
    assert_eq!(no_such_annex["error"], "not-found", "{no_such_annex}");
    let no_such_quote =
        domain::check_quote(&ctx, LSV_VERSION, "annex_99", "Belastungsgrenzwerte", None)
            .expect("runs");
    assert_eq!(no_such_quote["error"], "not-found", "{no_such_quote}");
    // Refusals, each with its true reason: a place that is not there;
    // a structural element; an unknown eId shape; a bad eId.
    let missing = domain::cite(&ctx, BGOE_VERSION, "art_999", None).expect("runs");
    assert_eq!(missing["error"], "not-found", "{missing}");
    let section = domain::cite(&ctx, BGOE_VERSION, "sec_1", None).expect("runs");
    assert_eq!(section["error"], "invalid-input", "{section}");
    assert!(section["detail"]
        .as_str()
        .unwrap()
        .contains("structural element"));
    let unknown = domain::cite(&ctx, BGOE_VERSION, "xyz_1", None).expect("runs");
    assert_eq!(unknown["error"], "invalid-input", "{unknown}");
    assert!(unknown["detail"]
        .as_str()
        .unwrap()
        .contains("no citation grammar yet"));
    assert!(!unknown["detail"]
        .as_str()
        .unwrap()
        .contains("structural element"));
    let bad = domain::cite(&ctx, BGOE_VERSION, "../x", None).expect("runs");
    assert_eq!(bad["error"], "invalid-input");
    // An inserted letter is a real place, labelled with its suffix.
    let suffixed = domain::cite(&ctx, &kvg_version, "art_25/para_2/lbl_f_bis", None).expect("runs");
    assert_eq!(suffixed["kind"], "norm", "{suffixed}");
    assert_eq!(suffixed["letter"], "fbis");
    assert_eq!(suffixed["designation"], "article");
}

/// A transitional provision (`disp_u<n>`, the KVG carries 25) is cited
/// by its heading — verbatim, the act's abbreviation appended — and
/// says so; a structural element is refused as one; the two must never
/// share a reason.
#[test]
fn cite_labels_a_transitional_provision_by_its_heading() {
    let ctx = fixture_ctx();
    let kvg_version = recorded_version_for(KVG).expect("a recorded KVG manifestation");
    let provision = domain::cite(&ctx, &kvg_version, "disp_u1", None).expect("runs");
    assert_eq!(provision["kind"], "norm", "{provision}");
    assert_eq!(provision["designation"], "transitional-provision");
    assert_eq!(
        provision["label"], "Schlussbestimmungen der Änderung vom 24. März 2000 KVG",
        "the exact label, not a prefix: {provision}"
    );
    assert!(provision["article"].is_null());
    assert_eq!(provision["element_kind"], "proviso");
    let paragraph = domain::cite(&ctx, &kvg_version, "disp_u1/para", None).expect("runs");
    assert_eq!(
        paragraph["designation"], "transitional-provision",
        "{paragraph}"
    );
    assert_eq!(
        paragraph["label"], "Schlussbestimmungen der Änderung vom 24. März 2000 KVG",
        "an unnumbered paragraph adds nothing, exactly as under an article: {paragraph}"
    );
    // A letter below a transitional provision is written in the article
    // branch's own grammar — two places, two labels (BT′ audit: before
    // this they shared one).
    for (eid, label) in [
        (
            "disp_u5/para/lbl_a",
            "Übergangsbestimmungen zur Änderung vom 21. Dezember 2007 (Spitalfinanzierung) Bst. a KVG",
        ),
        (
            "disp_u9/para/lbl_b",
            "Übergangsbestimmungen zur Änderung vom 19. März 2010 Bst. b KVG",
        ),
    ] {
        let out = domain::cite(&ctx, &kvg_version, eid, None).expect("runs");
        assert_eq!(out["label"], label, "{eid}: {out}");
        assert_eq!(out["designation"], "transitional-provision");
        assert!(out["letter"].is_null(), "the letter is in the label, not in the article fields: {out}");
    }
    // X17.3: the other arm — the elements that get no label are refused
    // with their TRUE reason, and the two reasons never merge.
    let section = domain::cite(&ctx, BGOE_VERSION, "sec_1", None).expect("runs");
    assert_eq!(section["error"], "invalid-input", "{section}");
    assert!(
        section["detail"]
            .as_str()
            .unwrap()
            .contains("structural element"),
        "{section}"
    );
    let unknown = domain::cite(&ctx, BGOE_VERSION, "xyz_1", None).expect("runs");
    assert!(
        unknown["detail"]
            .as_str()
            .unwrap()
            .contains("no citation grammar yet"),
        "{unknown}"
    );
    assert!(
        !unknown["detail"]
            .as_str()
            .unwrap()
            .contains("structural element"),
        "the two reasons never merge: {unknown}"
    );
    // The label has no grammar to parse back — said in the note, and true.
    assert!(provision["note"]
        .as_str()
        .unwrap()
        .contains("parse_reference has no grammar for that label"));
    let parsed = domain::parse_reference(&ctx, provision["label"].as_str().unwrap()).expect("runs");
    assert!(
        parsed["references"][0]["eid_candidate"].is_null(),
        "{parsed}"
    );
}

/// The round trip: cite → label → parse_reference → the same eId (and
/// the same act) — the two directions of the one grammar agree on
/// twelve addresses (two with a Latin-suffixed letter, «Bst. fbis» and
/// «Bst. gbis»), and an annex label comes back as its annex prefix.
#[test]
fn cite_and_parse_reference_round_trip() {
    let ctx = fixture_ctx();
    let kvg_version = recorded_version_for(KVG).expect("a recorded KVG manifestation");
    let addresses = [
        (BGOE_VERSION, "art_6"),
        (BGOE_VERSION, "art_6/para_1"),
        (BGOE_VERSION, "art_17"),
        (BGOE_VERSION, "art_23_a"),
        (BGOE_VERSION, "art_2/para_1"),
        (LSV_VERSION, "art_7"),
        (LSV_VERSION, "art_7/para_1/lbl_b"),
        (LSV_VERSION, "art_4/para_2"),
        (kvg_version.as_str(), "art_25_a"),
        (kvg_version.as_str(), "art_25_a/para_1/lbl_b"),
        (kvg_version.as_str(), "art_25/para_2/lbl_f_bis"),
        (kvg_version.as_str(), "art_84_a/para_1/lbl_g_bis"),
    ];
    for (version, eid) in addresses {
        let cited = domain::cite(&ctx, version, eid, Some("de")).expect("runs");
        let label = cited["label"].as_str().unwrap_or_else(|| panic!("{cited}"));
        let parsed = domain::parse_reference(&ctx, label).expect("runs");
        let reference = &parsed["references"][0];
        assert_eq!(
            reference["eid_candidate"], eid,
            "{label} must parse back to {eid}: {parsed}"
        );
        assert_eq!(reference["act"]["sr"], cited["sr"], "{label}: {parsed}");
        assert_eq!(reference["unresolved"], false, "{label}: {parsed}");
    }
    let annex = domain::cite(&ctx, LSV_VERSION, "annex_3/lvl_u1/lvl_2", Some("de")).expect("runs");
    let parsed = domain::parse_reference(&ctx, annex["label"].as_str().unwrap()).expect("runs");
    assert_eq!(parsed["references"][0]["kind"], "annex");
    assert_eq!(parsed["references"][0]["annex"], "3");
    assert!(parsed["references"][0]["annex_hint"]
        .as_str()
        .unwrap()
        .starts_with("annex_3"));
    assert_eq!(parsed["references"][0]["act"]["sr"], "814.41");
}

/// A quote check after a read costs the brake nothing: the same cache
/// line answers both.
#[test]
fn a_quote_check_after_a_read_takes_no_token() {
    let (ctx, clock, selects, fetches) = braked_ctx(2.0, 4.0, 5);
    let read = domain::read_article(&ctx, BGOE_VERSION, "art_6", Some("de")).expect("runs");
    assert_eq!(read["provenance"]["served"], "live");
    let brake = ctx.backend.throttle().expect("braked");
    assert_eq!(brake.admitted(), 2);
    let checked = domain::check_quote(
        &ctx,
        BGOE_VERSION,
        "art_6/para_1",
        "Jede Person hat das Recht",
        Some("de"),
    )
    .expect("runs");
    assert_eq!(checked["verified"], true, "{checked}");
    assert_eq!(checked["provenance"]["served"], "cache");
    assert_eq!(brake.admitted(), 2, "no token for a cached check");
    assert!(clock.sleeps().is_empty());
    assert_eq!(
        std::sync::atomic::AtomicUsize::load(&selects, std::sync::atomic::Ordering::SeqCst),
        1
    );
    assert_eq!(
        std::sync::atomic::AtomicUsize::load(&fetches, std::sync::atomic::Ordering::SeqCst),
        1
    );
    // cite takes ONE token: the abbreviation/SR query; the manifestation is cached.
    let cited = domain::cite(&ctx, BGOE_VERSION, "art_6", Some("de")).expect("runs");
    assert_eq!(cited["label"], "Art. 6 BGÖ", "{cited}");
    assert_eq!(cited["provenance"]["served"], "cache");
    assert_eq!(brake.admitted(), 3);
}

/// BT: records the act profiles behind `cite` (abbreviations, titles,
/// SR of the BGÖ, LSV and KVG) and the Italian BGÖ manifestation for
/// the third label language:
/// `cargo test --test e2e record_fixtures_bt -- --ignored --nocapture --test-threads 1`
#[test]
#[ignore = "hits the live endpoint once per key; run deliberately"]
fn record_fixtures_bt() {
    let ctx = Ctx {
        backend: Backend::recording(FEDLEX_ENDPOINT, fixtures_dir()),
        today: "2026-08-21".into(),
    };
    let kvg_version = recorded_version_for(KVG).expect("a recorded KVG manifestation");
    for (version, eid, lang) in [
        (BGOE_VERSION, "art_6", "de"),
        (LSV_VERSION, "art_7/para_1/lbl_b", "de"),
        (kvg_version.as_str(), "art_25_a", "de"),
        (BGOE_VERSION, "art_6/para_1", "it"),
    ] {
        let out = domain::cite(&ctx, version, eid, Some(lang)).expect("record");
        println!(
            "cite {eid} {lang}: {} ({})",
            out["label"], out["provenance"]["served"]
        );
    }
}

#[test]
fn every_stage_one_line_follows_the_house_rule() {
    let tools = FedlexServer::tool_router().list_all();
    assert_eq!(tools.len(), 35, "the thirty-five tools of TOOLSET-v1.md");
    // J11.3: the house rule is generic, so one line is held against its
    // own subject — get_drafts must say what a draft is good for.
    let drafts_line = tools
        .iter()
        .find(|t| t.name == "fedlex.get_drafts")
        .and_then(|t| t.description.as_deref())
        .expect("the get_drafts line");
    assert!(
        drafts_line.contains("use as the entry to consultations and materials"),
        "the line says «how did this come about»: {drafts_line}"
    );
    for tool in tools {
        let description = tool.description.as_deref().unwrap_or_default();
        summary_conforms(description)
            .unwrap_or_else(|why| panic!("{}: {why} — «{description}»", tool.name));
    }
}

// --- real stdio session over fixtures ---------------------------------

fn run_session(requests: &[String]) -> Vec<serde_json::Value> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_oh-mcp-fedlex"))
        .args(["--fixtures", fixtures_dir().to_str().unwrap()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn fedlex server");
    {
        let stdin = child.stdin.as_mut().expect("stdin");
        for request in requests {
            writeln!(stdin, "{request}").expect("write");
        }
    }
    let output = child.wait_with_output().expect("wait");
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("JSON-RPC line"))
        .collect()
}

#[test]
fn stdio_session_serves_the_toolset() {
    let responses = run_session(&[
        format!(
            r#"{{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{{"_meta":{STATELESS_META}}}}}"#
        ),
        format!(
            r#"{{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{{"name":"fedlex.resolve_sr","arguments":{{"sr":"832.10"}},"_meta":{STATELESS_META}}}}}"#
        ),
        format!(
            r#"{{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{{"name":"fedlex.search_text","arguments":{{"eli_version":"{BGOE_VERSION}","query":"Zugang","limit":2}},"_meta":{STATELESS_META}}}}}"#
        ),
    ]);
    let list = responses
        .iter()
        .find(|r| r["id"] == 1)
        .expect("tools/list answer");
    let names: Vec<&str> = list["result"]["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .map(|t| t["name"].as_str().expect("name"))
        .collect();
    assert_eq!(names.len(), 35, "the thirty-five tools of TOOLSET-v1.md");
    for expected in [
        "fedlex.resolve_sr",
        "fedlex.read_article",
        "fedlex.get_structure",
        "fedlex.search_text",
        "fedlex.read_document",
        "fedlex.get_references",
        "fedlex.get_modifications",
        "fedlex.list_annexes",
        "fedlex.get_article_history",
        "fedlex.get_subdivisions",
        "fedlex.get_taxonomy",
        "fedlex.list_expressions",
        "fedlex.resolve_vocabulary_label",
        "fedlex.find_related_topic",
        "fedlex.extract_tables",
        "fedlex.parse_reference",
        "fedlex.compare_versions",
        "fedlex.explore_node",
        "fedlex.detect_foreign_content",
        "fedlex.find_treaties",
        "fedlex.get_treaty_info",
        "fedlex.get_consultations",
        "fedlex.get_consultation_documents",
        "fedlex.get_oc_act",
        "fedlex.get_memorial",
        "fedlex.get_fga_documents",
        "fedlex.get_drafts",
    ] {
        assert!(names.contains(&expected), "{expected} is served");
    }

    let call = responses
        .iter()
        .find(|r| r["id"] == 2)
        .expect("call answer");
    let text = call["result"]["content"][0]["text"].as_str().expect("text");
    let payload: serde_json::Value = serde_json::from_str(text).expect("payload");
    assert_eq!(payload["eli"], KVG);

    let search = responses
        .iter()
        .find(|r| r["id"] == 3)
        .expect("search answer");
    let text = search["result"]["content"][0]["text"]
        .as_str()
        .expect("text");
    let payload: serde_json::Value = serde_json::from_str(text).expect("payload");
    assert_eq!(payload["kind"], "hint");
    assert_eq!(payload["hits"].as_array().unwrap().len(), 2);
}

// --- live smoke (deliberate) ------------------------------------------

/// One polite live request proving the recorded reality still holds.
/// `cargo test --test e2e live_smoke -- --ignored`
#[test]
#[ignore = "one live request against the public endpoint; run deliberately"]
fn live_smoke() {
    let ctx = Ctx {
        backend: Backend::live(FEDLEX_ENDPOINT),
        today: "2026-08-21".into(),
    };
    let out = domain::resolve_sr(&ctx, "832.10").expect("runs");
    assert_eq!(out["eli"], KVG);
}
