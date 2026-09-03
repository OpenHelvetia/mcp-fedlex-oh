//! Primitive: Änderungshistorie (Impacts) eines Erlasses (Rulebook J6).

use crate::client::{PREFIXES, SparqlClient, val};
use crate::{eli_uri, error::JoluxError};
use fedlex_core::{Eli, Provenance, Response, TransactionTime, ValidAsOf};
use serde::{Deserialize, Serialize};

/// Eine einzelne Änderung (`jolux:LegalResourceImpact`), die auf einen Erlass wirkt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Impact {
    /// URI des Impact-Knotens.
    pub impact_uri: String,
    /// Typ der Änderung (opake Vocabulary-URI: Änderung/Inkrafttreten/Aufhebung …).
    pub impact_type: Option<String>,
    /// Deutsches Label des Änderungstyps, direkt in der Query gejoint
    /// (68 §C-1) — erspart dem Konsumenten den `resolve_vocabulary_label`-
    /// Roundtrip pro URI.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub impact_type_label: Option<String>,
    /// Inkrafttreten der Änderung.
    pub date_entry_in_force: Option<String>,
    /// Freitext-Kommentar (seit 2023 oft die betroffenen Artikel, z.B. "Art. 5, 7").
    pub comment: Option<String>,
    /// Quell-Erlass der Änderung (OC-Änderungserlass).
    pub from: Option<String>,
}

// DISTINCT (68 §C-3): ?target ist gefiltert, aber nicht projiziert — ein
// Impact, der mehrere Subdivisions desselben Erlasses trifft, erzeugte sonst
// identische Zeilen (Join-Fanout; live beobachtet an Art. 19 EnG).
//
// **WAF-Zwang (live diagnostiziert 2026-07-03):** Der Fedlex-WAF blockiert
// Queries, die das SQL-Injection-Muster «SELECT … from» tragen, sobald die
// Query lang genug ist (~600 Zeichen; kürzere passieren) — sowohl `?from`
// als Variable wie auch `impactFromLegalResource` im Text zählen als
// Treffer. Deshalb sind die Hauptqueries komplett «from»-frei; die
// Quell-Erlasse holt eine ZWEITE, kurze Query ([`IMPACTS_SRC_Q`]), die
// unter der Schwelle bleibt (live verifiziert, 110 Zeilen am EnG). Der
// Regressions-Wächter dafür ist `waf_guard_main_queries_avoid_from`.
const IMPACTS_Q: &str = r#"SELECT DISTINCT ?impact ?type ?typeLabel ?date ?comment WHERE {
  ?impact jolux:impactToLegalResource ?target .
  OPTIONAL { ?impact jolux:legalResourceImpactHasType ?type
    OPTIONAL { ?type skos:prefLabel ?typeLabel . FILTER(LANG(?typeLabel) = "de") } }
  OPTIONAL { ?impact jolux:legalResourceImpactHasDateEntryInForce ?date }
  OPTIONAL { ?impact jolux:impactToLegalResourceComment ?comment }
  FILTER(STRSTARTS(STR(?target), "__URI__"))
} ORDER BY ?date"#;

/// Kurze Zweitquery: (impact → Quell-Erlass)-Paare. Bewusst minimal gehalten,
/// damit sie trotz «From»-Prädikat unter der WAF-Schwelle bleibt (s. o.).
const IMPACTS_SRC_Q: &str = r#"SELECT DISTINCT ?impact ?src WHERE {
  ?impact jolux:impactToLegalResource ?target ;
          jolux:impactFromLegalResource ?src .
  FILTER(STRSTARTS(STR(?target), "__URI__"))
}"#;

/// Listet die Änderungen (Impacts), die auf einen Erlass und seine Artikel wirken.
///
/// Liefert eine [`Response`] mit Provenance (die Historie *dieses* Erlasses).
///
/// Caveat (Rulebook J6.4): Seit 2023 dominiert wieder die **Freitext-Methode** —
/// betroffene Artikel stehen dann im `comment` ("Art. 5, 7, 12") statt in
/// strukturierten Subdivisions. Diese erste Fassung liefert die rohen Impacts;
/// das Parsen der Comment-Strings ist ein Folgeschritt, gegen Live-Daten zu
/// validieren.
pub async fn get_impacts(
    client: &impl SparqlClient,
    eli: &Eli,
    as_of: ValidAsOf,
) -> Result<Response<Vec<Impact>>, JoluxError> {
    let uri = eli_uri(eli);
    let sparql = format!("{PREFIXES}{}", IMPACTS_Q.replace("__URI__", &uri));
    let res = client.query(&sparql).await?;

    let src_sparql = format!("{PREFIXES}{}", IMPACTS_SRC_Q.replace("__URI__", &uri));
    let sources = source_map(&client.query(&src_sparql).await?);

    let impacts = dedup_impacts(res.bindings().iter().filter_map(|b| {
        let impact_uri = val(b, "impact")?.to_string();
        let from = sources.get(&impact_uri).cloned();
        Some(Impact {
            impact_uri,
            impact_type: val(b, "type").map(str::to_string),
            impact_type_label: nonempty(val(b, "typeLabel")),
            date_entry_in_force: val(b, "date").map(str::to_string),
            comment: nonempty(val(b, "comment")),
            from,
        })
    }));

    let prov = Provenance::new(eli.clone(), as_of, TransactionTime::now());
    Ok(Response::new(impacts, prov))
}

/// (impact → Quell-Erlass)-Zuordnung aus der Zweitquery; erste Quelle gewinnt.
fn source_map(res: &crate::client::SparqlResults) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    for b in res.bindings() {
        if let (Some(impact), Some(src)) = (val(b, "impact"), val(b, "src")) {
            map.entry(impact.to_string())
                .or_insert_with(|| src.to_string());
        }
    }
    map
}

/// Leere Literale werden zu `None` — ein `comment: ""` trägt keine Information
/// und gaukelt dem Konsumenten ein Feld vor (68 §C-3).
fn nonempty(v: Option<&str>) -> Option<String> {
    v.filter(|s| !s.is_empty()).map(str::to_string)
}

/// Defensive Dedup zusätzlich zum SPARQL-`DISTINCT` (68 §C-3): Fedlex-Daten
/// liefern gelegentlich inhaltsgleiche Impact-Zeilen über verschiedene
/// Graph-Pfade. Reihenfolge-erhaltend; die Listen sind klein.
fn dedup_impacts(iter: impl Iterator<Item = Impact>) -> Vec<Impact> {
    let mut out: Vec<Impact> = Vec::new();
    for imp in iter {
        if !out.contains(&imp) {
            out.push(imp);
        }
    }
    out
}

/// Normalisiert eine AKN-eId in die JOLux-Subdivision-Schreibweise (J18.2).
///
/// Die Regel lebt in `fedlex-core` ([`fedlex_core::normalize_eid`]) — hier
/// nur re-exportiert, damit die JOLux-API stabil bleibt.
pub use fedlex_core::normalize_eid;

// «from»-frei aus WAF-Gründen — Quell-Erlasse via [`ARTICLE_SRC_Q`] (s. IMPACTS_Q).
const ARTICLE_HISTORY_Q: &str = r#"SELECT DISTINCT ?impact ?type ?typeLabel ?date ?comment WHERE {
  ?impact jolux:impactToLegalResource ?target .
  OPTIONAL { ?impact jolux:legalResourceImpactHasType ?type
    OPTIONAL { ?type skos:prefLabel ?typeLabel . FILTER(LANG(?typeLabel) = "de") } }
  OPTIONAL { ?impact jolux:legalResourceImpactHasDateEntryInForce ?date }
  OPTIONAL { ?impact jolux:impactToLegalResourceComment ?comment }
  FILTER(STRSTARTS(STR(?target), "__URI__/") && CONTAINS(STR(?target), "__EID__"))
} ORDER BY ?date"#;

/// Kurze Zweitquery analog [`IMPACTS_SRC_Q`], gefiltert auf die Artikel-eId.
const ARTICLE_SRC_Q: &str = r#"SELECT DISTINCT ?impact ?src WHERE {
  ?impact jolux:impactToLegalResource ?target ;
          jolux:impactFromLegalResource ?src .
  FILTER(STRSTARTS(STR(?target), "__URI__/") && CONTAINS(STR(?target), "__EID__"))
}"#;

/// JLX-IMP-02: Änderungshistorie eines einzelnen Artikels.
///
/// Einzige Quelle für **historisch aufgehobene Artikel** — das aktuelle
/// AKN-XML enthält sie nicht mehr (J6.5). Die eId wird vor der Suche
/// normalisiert ([`normalize_eid`], J18.2). Nach dem Systembruch 2023 sind
/// Änderungen oft nur im `comment` des Gesamterlass-Impacts auffindbar
/// (J6.4) — eine leere Liste hier ist also **kein** Beweis für
/// "nie geändert"; ergänzend [`get_impacts`] + Comment-Parsing prüfen.
pub async fn get_article_history(
    client: &impl SparqlClient,
    eli: &Eli,
    eid: &str,
    as_of: ValidAsOf,
) -> Result<Response<Vec<Impact>>, JoluxError> {
    let normalized = normalize_eid(eid).replace(['"', '\\', '<', '>'], "");
    let sparql = format!(
        "{PREFIXES}{}",
        ARTICLE_HISTORY_Q
            .replace("__URI__", &eli_uri(eli))
            .replace("__EID__", &normalized)
    );
    let res = client.query(&sparql).await?;

    let src_sparql = format!(
        "{PREFIXES}{}",
        ARTICLE_SRC_Q
            .replace("__URI__", &eli_uri(eli))
            .replace("__EID__", &normalized)
    );
    let sources = source_map(&client.query(&src_sparql).await?);

    let impacts = dedup_impacts(res.bindings().iter().filter_map(|b| {
        let impact_uri = val(b, "impact")?.to_string();
        let from = sources.get(&impact_uri).cloned();
        Some(Impact {
            impact_uri,
            impact_type: val(b, "type").map(str::to_string),
            impact_type_label: nonempty(val(b, "typeLabel")),
            date_entry_in_force: val(b, "date").map(str::to_string),
            comment: nonempty(val(b, "comment")),
            from,
        })
    }));
    let prov = Provenance::new(eli.clone(), as_of, TransactionTime::now());
    Ok(Response::new(impacts, prov))
}

/// Eine ausgehende Änderung: dieser Erlass ändert ein anderes Gesetz.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutgoingImpact {
    /// URI des Impact-Knotens.
    pub impact_uri: String,
    /// Ziel der Änderung (CC-Gesetz oder Subdivision).
    pub target: String,
    /// Typ der Änderung (opake Vokabular-URI), sofern vorhanden.
    pub impact_type: Option<String>,
    /// Inkrafttreten der Änderung.
    pub date_entry_in_force: Option<String>,
}

// `?src` statt `?from` und kompakt gehalten: die Query bleibt so unter der
// WAF-Schwelle (s. IMPACTS_Q; live verifiziert, 69 Zeilen am EnG-OC).
const OUTGOING_Q: &str = r#"SELECT DISTINCT ?impact ?target ?type ?date WHERE {
  ?impact jolux:impactFromLegalResource ?src ;
          jolux:impactToLegalResource ?target .
  OPTIONAL { ?impact jolux:legalResourceImpactHasType ?type }
  OPTIONAL { ?impact jolux:legalResourceImpactHasDateEntryInForce ?date }
  FILTER(STRSTARTS(STR(?src), "__URI__"))
} ORDER BY ?date"#;

/// JLX-IMP-03: Welche Gesetze ändert dieser Erlass? (Richtung umgekehrt zu
/// [`get_impacts`]).
///
/// Traverse-out über `impactFromLegalResource`. Nur OC/FGA-Erlasse sind
/// Impact-Quellen, nie CC-Einträge — als `oc_eli` also `eli/oc/...`
/// übergeben (J8.4). Mantelerlasse bündeln viele Ziele.
pub async fn get_outgoing_impacts(
    client: &impl SparqlClient,
    oc_eli: &Eli,
    as_of: ValidAsOf,
) -> Result<Response<Vec<OutgoingImpact>>, JoluxError> {
    let sparql = format!(
        "{PREFIXES}{}",
        OUTGOING_Q.replace("__URI__", &eli_uri(oc_eli))
    );
    let res = client.query(&sparql).await?;
    let impacts = res
        .bindings()
        .iter()
        .filter_map(|b| {
            Some(OutgoingImpact {
                impact_uri: val(b, "impact")?.to_string(),
                target: val(b, "target")?.to_string(),
                impact_type: val(b, "type").map(str::to_string),
                date_entry_in_force: val(b, "date").map(str::to_string),
            })
        })
        .collect();
    let prov = Provenance::new(oc_eli.clone(), as_of, TransactionTime::now());
    Ok(Response::new(impacts, prov))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::MockSparqlClient;
    use time::macros::date;

    const FIXTURE: &str = r#"{
      "head": {"vars": ["impact","type","typeLabel","date","comment","src"]},
      "results": {"bindings": [
        {"impact":{"type":"uri","value":"https://fedlex.data.admin.ch/eli/impact/a1"},
         "type":{"type":"uri","value":"https://fedlex.data.admin.ch/vocabulary/impact-of-a-legal-resource-type/1"},
         "typeLabel":{"type":"literal","xml:lang":"de","value":"Änderung"},
         "date":{"type":"literal","value":"2020-06-01"},
         "comment":{"type":"literal","value":"Art. 5, 7, 12"}},
        {"impact":{"type":"uri","value":"https://fedlex.data.admin.ch/eli/impact/a2"},
         "date":{"type":"literal","value":"2023-01-01"},
         "src":{"type":"uri","value":"https://fedlex.data.admin.ch/eli/oc/2022/700"}}
      ]}
    }"#;

    #[tokio::test]
    async fn lists_impacts_with_comment_and_provenance() {
        let client = MockSparqlClient::from_json(FIXTURE);
        let eli = Eli::new("eli/cc/2017/762").unwrap();
        let resp = get_impacts(&client, &eli, ValidAsOf::new(date!(2024 - 01 - 01)))
            .await
            .unwrap();

        assert_eq!(resp.data().len(), 2);
        assert_eq!(resp.data()[0].comment.as_deref(), Some("Art. 5, 7, 12"));
        // 68 §C-1: Typ-Label direkt gejoint.
        assert_eq!(
            resp.data()[0].impact_type_label.as_deref(),
            Some("Änderung")
        );
        assert_eq!(
            resp.data()[0].date_entry_in_force.as_deref(),
            Some("2020-06-01")
        );
        assert!(
            resp.data()[1]
                .from
                .as_deref()
                .unwrap()
                .contains("eli/oc/2022/700")
        );

        // Provenance = Historie dieses Erlasses.
        assert_eq!(resp.provenance().eli.as_str(), "eli/cc/2017/762");

        // Query filtert auf Artikel-URIs des Erlasses.
        let q = client.last_query().unwrap();
        assert!(q.contains(
            r#"STRSTARTS(STR(?target), "https://fedlex.data.admin.ch/eli/cc/2017/762")"#
        ));
        assert!(q.contains("jolux:impactToLegalResource"));
    }

    #[tokio::test]
    async fn no_impacts_is_empty_list_not_error() {
        let empty = r#"{"head":{"vars":["impact"]},"results":{"bindings":[]}}"#;
        let client = MockSparqlClient::from_json(empty);
        let eli = Eli::new("eli/cc/1999/404").unwrap();
        let resp = get_impacts(&client, &eli, ValidAsOf::new(date!(2024 - 01 - 01)))
            .await
            .unwrap();
        assert!(resp.data().is_empty());
        assert_eq!(resp.provenance().eli.as_str(), "eli/cc/1999/404");
    }

    /// WAF-Wächter (live diagnostiziert 2026-07-03): Der Fedlex-WAF blockiert
    /// lange Queries mit dem SQL-Injection-Muster «SELECT … from» — die
    /// HAUPT-Queries müssen deshalb komplett «from»-frei bleiben (Variable
    /// UND Prädikat); die Quell-Erlasse holen die kurzen SRC-Queries.
    #[test]
    fn waf_guard_main_queries_avoid_from() {
        for (name, q) in [
            ("IMPACTS_Q", IMPACTS_Q),
            ("ARTICLE_HISTORY_Q", ARTICLE_HISTORY_Q),
        ] {
            assert!(
                !q.to_lowercase().contains("from"),
                "{name} muss «from»-frei bleiben (WAF), enthaelt: {q}"
            );
        }
        // Die SRC-Queries tragen das Prädikat zwangsläufig — sie müssen dafür
        // kurz bleiben (WAF-Schwelle empirisch ~600 Zeichen inkl. Prefixes).
        for (name, q) in [
            ("IMPACTS_SRC_Q", IMPACTS_SRC_Q),
            ("ARTICLE_SRC_Q", ARTICLE_SRC_Q),
        ] {
            assert!(
                PREFIXES.len() + q.len() + 100 < 600,
                "{name} zu lang fuer die WAF-Schwelle: {} Zeichen",
                PREFIXES.len() + q.len()
            );
        }
    }

    #[test]
    fn eid_normalization_follows_j18_2() {
        assert_eq!(normalize_eid("art_14_a"), "art_14a");
        assert_eq!(normalize_eid("art_2_b/para_1"), "art_2b/para_1");
        assert_eq!(normalize_eid("art_14"), "art_14"); // Ziffern unverändert
        assert_eq!(normalize_eid("annex_1"), "annex_1");
    }

    /// 68 §C-3: inhaltsgleiche Zeilen (Join-Fanout über nicht projizierte
    /// Filter-Variablen; live an Art. 19 EnG beobachtet) dürfen den Konsumenten
    /// nie erreichen — weder aus der Query (DISTINCT) noch aus den Daten
    /// (defensive Dedup). Leere comment-Literale werden zu None.
    #[tokio::test]
    async fn duplicate_rows_and_empty_comments_are_cleaned() {
        let fixture_with_dupes = r#"{
          "head": {"vars": ["impact","type","date","comment","from"]},
          "results": {"bindings": [
            {"impact":{"type":"uri","value":"https://fedlex.data.admin.ch/eli/impact/a1"},
             "date":{"type":"literal","value":"2023-01-01"},
             "comment":{"type":"literal","value":""}},
            {"impact":{"type":"uri","value":"https://fedlex.data.admin.ch/eli/impact/a1"},
             "date":{"type":"literal","value":"2023-01-01"},
             "comment":{"type":"literal","value":""}},
            {"impact":{"type":"uri","value":"https://fedlex.data.admin.ch/eli/impact/a1"},
             "date":{"type":"literal","value":"2024-01-01"},
             "comment":{"type":"literal","value":""}}
          ]}
        }"#;
        let client = MockSparqlClient::from_json(fixture_with_dupes);
        let eli = Eli::new("eli/cc/2017/762").unwrap();

        let resp = get_article_history(
            &client,
            &eli,
            "art_19",
            ValidAsOf::new(date!(2026 - 07 - 02)),
        )
        .await
        .unwrap();
        // Zeile 1+2 sind identisch → eine bleibt; Zeile 3 (anderes Datum) bleibt.
        assert_eq!(resp.data().len(), 2, "identische Zeilen müssen kollabieren");
        assert_eq!(
            resp.data()[0].comment,
            None,
            "leeres comment-Literal muss None sein"
        );

        // Beide Impact-Queries dedupen bereits an der Quelle.
        let q = client.last_query().unwrap();
        assert!(q.contains("SELECT DISTINCT"), "Query ohne DISTINCT: {q}");

        let resp = get_impacts(&client, &eli, ValidAsOf::new(date!(2026 - 07 - 02)))
            .await
            .unwrap();
        assert_eq!(resp.data().len(), 2);
        assert!(client.last_query().unwrap().contains("SELECT DISTINCT"));
    }

    #[tokio::test]
    async fn article_history_normalizes_eid_in_query() {
        let client = MockSparqlClient::from_json(FIXTURE);
        let eli = Eli::new("eli/cc/2017/762").unwrap();
        let resp = get_article_history(
            &client,
            &eli,
            "art_14_a",
            ValidAsOf::new(date!(2026 - 01 - 01)),
        )
        .await
        .unwrap();
        assert_eq!(resp.data().len(), 2);

        let q = client.last_query().unwrap();
        assert!(
            q.contains(r#"CONTAINS(STR(?target), "art_14a")"#),
            "eId nicht normalisiert: {q}"
        );
        assert!(q.contains("https://fedlex.data.admin.ch/eli/cc/2017/762/"));
    }

    #[tokio::test]
    async fn outgoing_impacts_filter_on_source() {
        let client = MockSparqlClient::from_json(
            r#"{"head":{"vars":["impact","target","type","date"]},"results":{"bindings":[
              {"impact":{"type":"uri","value":"https://fedlex.data.admin.ch/eli/impact/x1"},
               "target":{"type":"uri","value":"https://fedlex.data.admin.ch/eli/cc/1998/3033/art_7"},
               "date":{"type":"literal","value":"2018-01-01"}}
            ]}}"#,
        );
        let oc = Eli::new("eli/oc/2017/762").unwrap();
        let resp = get_outgoing_impacts(&client, &oc, ValidAsOf::new(date!(2026 - 01 - 01)))
            .await
            .unwrap();
        assert_eq!(resp.data().len(), 1);
        assert!(resp.data()[0].target.contains("eli/cc/1998/3033"));

        let q = client.last_query().unwrap();
        assert!(
            q.contains(r#"STRSTARTS(STR(?src), "https://fedlex.data.admin.ch/eli/oc/2017/762")"#),
            "OUTGOING_Q filtert ueber ?src (WAF-sicher): {q}"
        );
    }
}
