//! Primitive: Identitäts-Auflösung — SR-Nummer, Sprachen, Manifestationen
//! (Lexikon JLX-RES-01/04/05, Rulebook J2/J13).

use crate::client::{Language, PREFIXES, SparqlClient, val};
use crate::{FEDLEX_BASE, eli_uri, error::JoluxError};
use fedlex_core::{Eli, Provenance, Response, TransactionTime, ValidAsOf};
use serde::{Deserialize, Serialize};

/// Ein Treffer der SR-Nummern-Auflösung.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SrHit {
    /// ELI des Erlasses (relativ, `eli/cc/...`).
    pub eli: String,
    /// Titel in der angefragten Sprache, sofern vorhanden.
    pub title: Option<String>,
    /// `jolux:inForceStatus` (opake Vokabular-URI), sofern vorhanden.
    /// Achtung: der Status beschreibt IMMER die heutige Geltung, nicht die
    /// zum Stichtag — dafür ist `in_force` da.
    pub in_force_status: Option<String>,
    /// Deutsches Label zum Status (z. B. «In Kraft»), direkt gejoint
    /// (68 §F-27 — check_in_force liefert dasselbe als current_status_label;
    /// eine nackte Vokabular-URI musste vorher separat aufgelöst werden).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub in_force_status_label: Option<String>,
    /// Abgeleitete Geltung **zum Stichtag `as_of`** (68 §C-5/F-3): das
    /// Disambiguierungs-Kriterium bei wiederverwendeten SR-Nummern, direkt
    /// als Flag statt als zu deutende URI. Ohne Datumsfelder am Erlass nur
    /// für den heutigen Stichtag ableitbar, sonst `None` (nie geraten).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub in_force: Option<bool>,
}

// Zwei Pfade zur SR-Nummer (Live-Befund 2026-07-03): `historicalLegalId`
// tragen nur ältere Erlasse — das geltende nDSG (`eli/cc/2022/491`, SR 235.1)
// hat **kein** SR-Literal am Erlass und ist nur über die Systematik-Taxonomie
// (`skos:notation`, typisiert als `notation-type/id-systematique`) auffindbar.
// Ohne den zweiten Pfad landete "SR 235.1" ausschliesslich auf dem
// aufgehobenen DSG von 1992 — ohne Zeiger auf das geltende Recht.
const SR_Q: &str = r#"SELECT DISTINCT ?ca ?title ?status ?statusLabel ?entry ?noLonger ?endApp WHERE {
  { ?ca a jolux:ConsolidationAbstract ;
        jolux:historicalLegalId "__SR__" . }
  UNION
  { ?tax skos:notation "__SR__"^^<https://fedlex.data.admin.ch/vocabulary/notation-type/id-systematique> .
    ?ca jolux:classifiedByTaxonomyEntry ?tax ;
        a jolux:ConsolidationAbstract . }
  OPTIONAL {
    ?ca jolux:isRealizedBy ?expr .
    ?expr jolux:language <__LANGURI__> ; jolux:title ?title .
  }
  OPTIONAL { ?ca jolux:inForceStatus ?status
    OPTIONAL { ?status skos:prefLabel ?statusLabel . FILTER(LANG(?statusLabel) = "de") } }
  OPTIONAL { ?ca jolux:dateEntryInForce ?entry }
  OPTIONAL { ?ca jolux:dateNoLongerInForce ?noLonger }
  OPTIONAL { ?ca jolux:dateEndApplicability ?endApp }
} LIMIT 20"#;

/// JLX-RES-01: Löst eine SR-Nummer zu den passenden Erlassen auf.
///
/// **Liefert eine Liste**, denn SR-Nummern werden wiederverwendet — 730.0
/// zeigt auf das alte EnG (`eli/cc/1999/27`, aufgehoben) *und* das neue
/// (`eli/cc/2017/762`). Geltende Erlasse stehen zuerst; Disambiguierung über
/// `in_force` bzw. [`check_in_force`]. Gefunden wird über `historicalLegalId`
/// **und** die Systematik-Taxonomie — neue Konsolidierungen (z.B. nDSG)
/// tragen kein SR-Literal mehr am Erlass. Live-verifiziert 2026-07-03.
///
/// Discovery-Funktion ohne Provenance. Die SR-Nummer wird entschärft
/// eingebettet (keine SPARQL-Injection).
///
/// [`check_in_force`]: crate::temporal::check_in_force
pub async fn resolve_sr_number(
    client: &impl SparqlClient,
    sr_number: &str,
    lang: Language,
    as_of: ValidAsOf,
) -> Result<Vec<SrHit>, JoluxError> {
    let safe = sr_number.replace(['"', '\\'], " ");
    let sparql = format!(
        "{PREFIXES}{}",
        SR_Q.replace("__SR__", &safe)
            .replace("__LANGURI__", lang.vocab_uri())
    );
    let res = client.query(&sparql).await?;
    let mut hits: Vec<SrHit> = Vec::new();
    for b in res.bindings() {
        let Some(ca) = val(b, "ca") else { continue };
        let hit = SrHit {
            eli: ca.strip_prefix(FEDLEX_BASE).unwrap_or(ca).to_string(),
            title: val(b, "title").map(str::to_string),
            in_force: crate::search::in_force_at(
                val(b, "status"),
                val(b, "entry"),
                val(b, "noLonger"),
                val(b, "endApp"),
                as_of,
            ),
            in_force_status: val(b, "status").map(str::to_string),
            in_force_status_label: val(b, "statusLabel")
                .filter(|s| !s.is_empty())
                .map(str::to_string),
        };
        // Dedup pro ELI (68 §F-19/Verify-V12): der UNION-Zweipfad
        // (historicalLegalId + Taxonomie) und Mehrfach-Expressions lieferten
        // denselben Erlass doppelt — live bei SR 818.102 sogar mit
        // widerspruechlichen Labels in einer Antwort. Erste Zeile gewinnt,
        // fehlende Felder werden nachgetragen (wie search_law::collect_hits).
        match hits.iter_mut().find(|h| h.eli == hit.eli) {
            Some(existing) => {
                if existing.title.is_none() {
                    existing.title = hit.title;
                }
                if existing.in_force.is_none() {
                    existing.in_force = hit.in_force;
                }
                if existing.in_force_status.is_none() {
                    existing.in_force_status = hit.in_force_status;
                    existing.in_force_status_label = hit.in_force_status_label;
                }
            }
            None => hits.push(hit),
        }
    }
    // Geltendes Recht zuerst (wie search_law verspricht) — bei
    // SR-Wiederverwendung ist der aufgehobene Alt-Erlass sonst der
    // erstbeste Treffer.
    hits.sort_by_key(|h| match h.in_force {
        Some(true) => 0,
        Some(false) => 1,
        None => 2,
    });
    Ok(hits)
}

/// Gewünschtes Manifestations-Format (Rulebook J19.5: XML/PDF/HTML/DOCX).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManifestationFormat {
    /// AKN-XML (ab ~2021 verfügbar, J14.2).
    Xml,
    /// PDF-A (vollständige Historie).
    Pdf,
    /// HTML.
    Html,
}

impl ManifestationFormat {
    /// Teil-String, über den die Exemplar-URL gefiltert wird.
    fn url_marker(self) -> &'static str {
        match self {
            ManifestationFormat::Xml => "xml",
            ManifestationFormat::Pdf => "pdf",
            ManifestationFormat::Html => "html",
        }
    }
}

/// Eine aufgelöste Manifestation (Download-URL einer Fassung).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifestation {
    /// Download-URL des Exemplars.
    pub url: String,
    /// Stand-Datum der zugrunde liegenden Fassung.
    pub consolidation_date: String,
    /// Sprache der Manifestation.
    pub language: String,
}

const MANIF_Q: &str = r#"SELECT ?date ?url WHERE {
  ?cons jolux:isMemberOf <__URI__> ;
        jolux:dateApplicability ?date ;
        jolux:isRealizedBy ?expr .
  ?expr jolux:language <__LANGURI__> ;
        jolux:isEmbodiedBy ?manif .
  ?manif jolux:isExemplifiedBy ?url .
  FILTER(CONTAINS(STR(?url), "__FMT__"))
  FILTER(?date <= xsd:date("__DATE__"))
} ORDER BY DESC(?date) LIMIT 1"#;
// xsd:date(…)-Konstruktor, nie "…"^^xsd:date-Literal (Betriebsregel
// Datumsvergleich, docs/dev/10_LEXICON_jolux.md).

/// JLX-RES-04: Liefert die Download-URL der zum Stichtag gültigen Fassung
/// im gewünschten Format.
///
/// FRBR-Kette `?cons isMemberOf <CA>` → `isRealizedBy` → `isEmbodiedBy` →
/// `isExemplifiedBy` (J2.1/J2.2). Die Richtung ist **eingehend** — die
/// Gegenrichtung liefert 0 Ergebnisse. XML existiert erst ab ~2021, ältere
/// Fassungen nur als PDF (J14.2) → dann [`JoluxError::NotFound`].
pub async fn resolve_manifestation(
    client: &impl SparqlClient,
    eli: &Eli,
    as_of: ValidAsOf,
    lang: Language,
    format: ManifestationFormat,
) -> Result<Response<Manifestation>, JoluxError> {
    let uri = eli_uri(eli);
    let sparql = format!(
        "{PREFIXES}{}",
        MANIF_Q
            .replace("__URI__", &uri)
            .replace("__LANGURI__", lang.vocab_uri())
            .replace("__FMT__", format.url_marker())
            .replace("__DATE__", &as_of.to_string())
    );
    let res = client.query(&sparql).await?;
    let b = res
        .bindings()
        .first()
        .ok_or_else(|| JoluxError::NotFound(uri.clone()))?;
    let manif = Manifestation {
        url: val(b, "url").unwrap_or_default().to_string(),
        consolidation_date: val(b, "date").unwrap_or_default().to_string(),
        language: lang.tag().to_string(),
    };
    let prov = Provenance::new(eli.clone(), as_of, TransactionTime::now());
    Ok(Response::new(manif, prov))
}

const EXPR_Q: &str = r#"SELECT DISTINCT ?lang WHERE {
  ?cons jolux:isMemberOf <__URI__> ;
        jolux:isRealizedBy ?expr .
  ?expr jolux:language ?lang .
} LIMIT 10"#;

/// JLX-RES-05: Listet die Sprachen, in denen ein Erlass vorliegt.
///
/// Liefert die **Sprach-Codes** (`de|fr|it|en|rm`) — dieselben Werte, die
/// alle anderen Primitive als `lang`-Parameter erwarten (68 §C-1: die rohen
/// EU-Vokabular-URIs waren für den Konsumenten nicht rückführbar). Fremde
/// URIs ausserhalb des Amtssprachen-Vokabulars werden roh durchgereicht
/// statt verschluckt. Reiner Helfer ohne Provenance.
pub async fn list_expressions(
    client: &impl SparqlClient,
    eli: &Eli,
) -> Result<Vec<String>, JoluxError> {
    let sparql = format!("{PREFIXES}{}", EXPR_Q.replace("__URI__", &eli_uri(eli)));
    let res = client.query(&sparql).await?;
    Ok(res
        .bindings()
        .iter()
        .filter_map(|b| val(b, "lang"))
        .map(|uri| match Language::from_vocab_uri(uri) {
            Some(l) => l.tag().to_string(),
            None => uri.to_string(),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::MockSparqlClient;
    use fedlex_core::swiss_today;
    use time::macros::date;

    #[tokio::test]
    async fn sr_number_returns_all_reused_hits() {
        let client = MockSparqlClient::from_json(
            r#"{"head":{"vars":["ca","title","status"]},"results":{"bindings":[
              {"ca":{"type":"uri","value":"https://fedlex.data.admin.ch/eli/cc/1999/27"},
               "status":{"type":"uri","value":"https://fedlex.data.admin.ch/vocabulary/enforcement-status/1"}},
              {"ca":{"type":"uri","value":"https://fedlex.data.admin.ch/eli/cc/2017/762"},
               "title":{"type":"literal","xml:lang":"de","value":"Energiegesetz (EnG)"},
               "status":{"type":"uri","value":"https://fedlex.data.admin.ch/vocabulary/enforcement-status/0"}}
            ]}}"#,
        );
        let hits = resolve_sr_number(
            &client,
            "730.0",
            Language::De,
            ValidAsOf::new(swiss_today()),
        )
        .await
        .unwrap();
        assert_eq!(hits.len(), 2, "SR-Wiederverwendung: Liste, kein Einzelwert");
        // Geltendes Recht zuerst — der aufgehobene Alt-Erlass folgt dahinter.
        assert_eq!(hits[0].eli, "eli/cc/2017/762");
        assert!(hits[0].in_force_status.as_deref().unwrap().ends_with("/0"));
        assert_eq!(hits[0].in_force, Some(true));
        assert_eq!(hits[1].eli, "eli/cc/1999/27");

        let q = client.last_query().unwrap();
        assert!(q.contains(r#"jolux:historicalLegalId "730.0""#));
        // Zweiter Pfad: Systematik-Taxonomie für Erlasse ohne SR-Literal
        // (Live-Befund 2026-07-03, nDSG).
        assert!(q.contains(r#"skos:notation "730.0"^^"#));
        assert!(q.contains("classifiedByTaxonomyEntry"));
        assert!(q.contains("SELECT DISTINCT"));
    }

    /// 68 §F-19/Verify-V12 (live bei SR 818.102): der UNION-Zweipfad lieferte
    /// dieselbe ELI doppelt, teils mit widerspruechlichen Status-Labels in
    /// EINER Antwort. Jetzt: ein Treffer pro ELI, Felder gemerged.
    #[tokio::test]
    async fn duplicate_ca_rows_collapse_and_merge() {
        let client = MockSparqlClient::from_json(
            r#"{"head":{"vars":["ca","title","status","statusLabel","entry"]},"results":{"bindings":[
              {"ca":{"type":"uri","value":"https://fedlex.data.admin.ch/eli/cc/1995/1328_1328_1328"},
               "entry":{"type":"literal","value":"1995-01-01"}},
              {"ca":{"type":"uri","value":"https://fedlex.data.admin.ch/eli/cc/1995/1328_1328_1328"},
               "title":{"type":"literal","xml:lang":"de","value":"Verordnung ueber die Krankenversicherung"},
               "status":{"type":"uri","value":"https://fedlex.data.admin.ch/vocabulary/enforcement-status/0"},
               "statusLabel":{"type":"literal","xml:lang":"de","value":"In Kraft"},
               "entry":{"type":"literal","value":"1995-01-01"}}
            ]}}"#,
        );
        let hits = resolve_sr_number(
            &client,
            "818.102",
            Language::De,
            ValidAsOf::new(swiss_today()),
        )
        .await
        .unwrap();
        assert_eq!(hits.len(), 1, "Duplikat nicht kollabiert: {hits:?}");
        assert!(hits[0].title.as_deref().unwrap().contains("Verordnung"));
        assert_eq!(hits[0].in_force_status_label.as_deref(), Some("In Kraft"));
    }

    #[tokio::test]
    async fn sr_number_neutralizes_injection() {
        let client =
            MockSparqlClient::from_json(r#"{"head":{"vars":["ca"]},"results":{"bindings":[]}}"#);
        let _ = resolve_sr_number(
            &client,
            r#"730" } INJECT {"#,
            Language::De,
            ValidAsOf::new(swiss_today()),
        )
        .await
        .unwrap();
        let q = client.last_query().unwrap();
        assert!(!q.contains("\" }"), "Breakout-Sequenz nicht neutralisiert");
    }

    #[tokio::test]
    async fn manifestation_filters_format_and_date() {
        let client = MockSparqlClient::from_json(
            r#"{"head":{"vars":["date","url"]},"results":{"bindings":[{
              "date":{"type":"literal","value":"2023-06-01"},
              "url":{"type":"uri","value":"https://fedlex.data.admin.ch/.../de/pdf-a/doc.pdf"}
            }]}}"#,
        );
        let eli = Eli::new("eli/cc/2017/762").unwrap();
        let resp = resolve_manifestation(
            &client,
            &eli,
            ValidAsOf::new(date!(2023 - 06 - 15)),
            Language::De,
            ManifestationFormat::Pdf,
        )
        .await
        .unwrap();
        assert!(resp.data().url.ends_with(".pdf"));
        assert_eq!(resp.data().consolidation_date, "2023-06-01");

        let q = client.last_query().unwrap();
        assert!(q.contains(r#"CONTAINS(STR(?url), "pdf")"#));
        assert!(q.contains(r#"?date <= xsd:date("2023-06-15")"#));
    }

    #[tokio::test]
    async fn manifestation_missing_format_is_not_found() {
        let client = MockSparqlClient::from_json(
            r#"{"head":{"vars":["date","url"]},"results":{"bindings":[]}}"#,
        );
        let eli = Eli::new("eli/cc/1907/233").unwrap();
        let err = resolve_manifestation(
            &client,
            &eli,
            ValidAsOf::new(date!(1950 - 01 - 01)),
            Language::De,
            ManifestationFormat::Xml,
        )
        .await
        .unwrap_err();
        assert!(matches!(err, JoluxError::NotFound(_)));
    }

    /// 68 §C-1: Die EU-Vokabular-URIs werden auf die eigenen Sprach-Codes
    /// gemappt — dieselben Werte, die jedes `lang`-Argument erwartet. Fremde
    /// URIs werden roh durchgereicht, nie verschluckt.
    #[tokio::test]
    async fn expressions_map_to_own_language_codes() {
        let client = MockSparqlClient::from_json(
            r#"{"head":{"vars":["lang"]},"results":{"bindings":[
              {"lang":{"type":"uri","value":"http://publications.europa.eu/resource/authority/language/DEU"}},
              {"lang":{"type":"uri","value":"http://publications.europa.eu/resource/authority/language/FRA"}},
              {"lang":{"type":"uri","value":"http://publications.europa.eu/resource/authority/language/ROH"}},
              {"lang":{"type":"uri","value":"http://publications.europa.eu/resource/authority/language/LAT"}}
            ]}}"#,
        );
        let eli = Eli::new("eli/cc/2017/762").unwrap();
        let langs = list_expressions(&client, &eli).await.unwrap();
        assert_eq!(
            langs,
            vec![
                "de",
                "fr",
                "rm",
                "http://publications.europa.eu/resource/authority/language/LAT"
            ]
        );
    }
}
