//! Primitive: Zitationsgraph (Lexikon JLX-CIT-01, Rulebook J7).
//!
//! JOLux-Zitationen existieren nur auf Gesamttext-Granularität (`/text`),
//! nie auf Artikel-Ebene (J7.1). Der Overlap mit AKN-Inline-`<ref>` beträgt
//! nur 0–48 % (J7.3) — vollständige Zitationsnetze brauchen den **Merge**
//! beider Quellen.
//!
//! **Query-Architektur (Live-Befund 2026-07-03):** Zitationen hängen an den
//! Fassungs-Text-URIs (`<eli>/text/<datum>`, ausgehend) bzw. der datumlosen
//! Gesamttext-URI (`<eli>/text`, eingehend). Die frühere Ein-Query-Form mit
//! `FILTER(STRSTARTS(…))` erzwang einen Full-Scan über den gesamten
//! Zitationsgraphen und lief bei Erlassen mit realem Zitationsnetz (DSG)
//! reproduzierbar in den 15-s-Timeout. Deshalb hier das Muster aus
//! [`crate::impacts`]: eine «from»-freie Hauptquery löst den Stichtag zur
//! Fassung auf, kurze Zweitqueries binden die Zitations-URIs **exakt**
//! (indexgestützt, < 0,5 s) und bleiben unter der WAF-Schwelle
//! ([Betriebsregel WAF](../docs/dev/10_LEXICON_jolux.md)).

use crate::client::{PREFIXES, SparqlClient, val};
use crate::{eli_uri, error::JoluxError};
use fedlex_core::{Eli, Provenance, Response, TransactionTime, ValidAsOf};
use serde::{Deserialize, Serialize};

/// Richtung der Zitations-Abfrage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CitationDirection {
    /// Was zitiert dieser Erlass? (`citationFromLegalResource` = Erlass)
    Outgoing,
    /// Wer zitiert diesen Erlass? (`citationToLegalResource` = Erlass)
    Incoming,
    /// Beide Richtungen (zwei kurze Queries, clientseitig gemergt).
    Both,
}

/// Eine formale Zitation zwischen zwei Erlassen.
///
/// `from`/`to` tragen die **Erlass-URIs** (ohne `/text`-Suffixe) — die
/// Fassungs-Duplikate des Graphen sind bereits nach Quellgesetz
/// dedupliziert (J7.4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Citation {
    /// Quelle der Zitation.
    pub from: String,
    /// Ziel der Zitation.
    pub to: String,
    /// Beschreibung der Fundstelle (seit ~2026 von Fedlex befüllt,
    /// Rulebook J7.2 überholt — trotzdem optional behandeln).
    pub description: Option<String>,
}

/// Hauptquery («from»-frei, WAF-Betriebsregel): löst Erlass + Stichtag zur
/// Gesamttext-Subdivision der jüngsten Fassung ≤ `as_of` auf. Ausgehende
/// Zitationen hängen in JOLux an genau diesen Fassungs-Text-URIs.
const CIT_VERSION_Q: &str = r#"SELECT ?sub WHERE {
  ?cons jolux:isMemberOf <__URI__> ;
        jolux:dateApplicability ?date .
  ?sub jolux:legalResourceSubdivisionIsPartOf ?cons ;
       jolux:legalResourceSubdivisionType <https://fedlex.data.admin.ch/vocabulary/subdivision-type/text> .
  FILTER(?date <= xsd:date("__DATE__"))
} ORDER BY DESC(?date) LIMIT 1"#;
// xsd:date(…)-Konstruktor, nie "…"^^xsd:date-Literal (Betriebsregel
// Datumsvergleich, docs/dev/10_LEXICON_jolux.md).

/// Kurze Zweitquery (Prädikat mit «From» — muss unter der WAF-Schwelle
/// bleiben): ausgehende Zitationen der aufgelösten Fassung, exakt gebunden.
/// Ohne DISTINCT (WAF-Vorfall 2026-06-10), Dedup clientseitig.
const CIT_OUT_Q: &str = r#"SELECT ?dst ?desc WHERE {
  ?cit jolux:citationFromLegalResource <__SUB__> ;
       jolux:citationToLegalResource ?dst .
  OPTIONAL { ?cit jolux:descriptionFrom ?desc }
}"#;

/// Kurze Zweitquery: eingehende Zitationen, exakt an die datumlose
/// Gesamttext-URI gebunden. Die Quellen sind Fassungs-URIs der zitierenden
/// Erlasse (eine pro Fassung — Dedup nach Quellgesetz clientseitig, J7.4).
/// Stichtags-Filterung der Quellseite wäre nur mit einem langen Join möglich,
/// der die WAF-Schwelle reisst — eingehend gilt daher „in irgendeiner
/// erfassten Fassung zitiert".
const CIT_IN_Q: &str = r#"SELECT ?src ?desc WHERE {
  ?cit jolux:citationToLegalResource <__URI__/text> ;
       jolux:citationFromLegalResource ?src .
  OPTIONAL { ?cit jolux:descriptionFrom ?desc }
}"#;

/// JLX-CIT-01: Formale Zitationen eines Erlasses (ein- und/oder ausgehend).
///
/// Ausgehend gilt die zum Stichtag `as_of` anwendbare Fassung (jüngste
/// `dateApplicability` ≤ `as_of` mit erfasster Text-Subdivision); ohne solche
/// Fassung ist das Ergebnis leer, kein Fehler. Dedupliziert nach
/// `(from, to)` auf Erlass-Ebene — der Graph führt Zitationen pro Fassung
/// mehrfach (J7.4). **Nicht** mit Vollständigkeit verwechseln: für das echte
/// Zitationsnetz JOLux ⊕ AKN-Refs mergen (J7.3).
pub async fn get_citations(
    client: &impl SparqlClient,
    eli: &Eli,
    direction: CitationDirection,
    as_of: ValidAsOf,
) -> Result<Response<Vec<Citation>>, JoluxError> {
    let uri = eli_uri(eli);
    let mut citations: Vec<Citation> = Vec::new();

    if matches!(
        direction,
        CitationDirection::Outgoing | CitationDirection::Both
    ) {
        let version_q = format!(
            "{PREFIXES}{}",
            CIT_VERSION_Q
                .replace("__URI__", &uri)
                .replace("__DATE__", &as_of.to_string())
        );
        let sub = client
            .query(&version_q)
            .await?
            .bindings()
            .first()
            .and_then(|b| val(b, "sub"))
            .map(str::to_string);
        if let Some(sub) = sub {
            let out_q = format!("{PREFIXES}{}", CIT_OUT_Q.replace("__SUB__", &sub));
            let res = client.query(&out_q).await?;
            for b in res.bindings() {
                let Some(dst) = val(b, "dst") else { continue };
                push_deduped(
                    &mut citations,
                    Citation {
                        from: uri.clone(),
                        to: act_uri(dst),
                        description: val(b, "desc").map(str::to_string),
                    },
                );
            }
        }
    }

    if matches!(
        direction,
        CitationDirection::Incoming | CitationDirection::Both
    ) {
        let in_q = format!("{PREFIXES}{}", CIT_IN_Q.replace("__URI__", &uri));
        let res = client.query(&in_q).await?;
        for b in res.bindings() {
            let Some(src) = val(b, "src") else { continue };
            push_deduped(
                &mut citations,
                Citation {
                    from: act_uri(src),
                    to: uri.clone(),
                    description: val(b, "desc").map(str::to_string),
                },
            );
        }
    }

    let prov = Provenance::new(eli.clone(), as_of, TransactionTime::now());
    Ok(Response::new(citations, prov))
}

/// Normalisiert eine Gesamttext-URI (`<eli>/text` bzw. `<eli>/text/<datum>`)
/// auf die Erlass-URI — Grundlage der Dedup nach Quellgesetz (J7.4).
fn act_uri(text_uri: &str) -> String {
    match text_uri.find("/text") {
        Some(i) => text_uri[..i].to_string(),
        None => text_uri.to_string(),
    }
}

/// Dedup nach `(from, to)`; eine später eintreffende Beschreibung füllt einen
/// noch leeren Eintrag auf (Fedlex liefert Duplikate teils ohne `desc`).
fn push_deduped(citations: &mut Vec<Citation>, new: Citation) {
    if let Some(existing) = citations
        .iter_mut()
        .find(|c| c.from == new.from && c.to == new.to)
    {
        if existing.description.is_none() {
            existing.description = new.description;
        }
        return;
    }
    citations.push(new);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::SparqlResults;
    use async_trait::async_trait;
    use std::sync::Mutex;
    use time::macros::date;

    /// Sequenz-Mock: liefert pro Query die nächste Fixture und protokolliert
    /// alle Queries (der Zwei-Query-Fluss braucht unterschiedliche Antworten).
    struct SeqMock {
        canned: Mutex<std::collections::VecDeque<SparqlResults>>,
        queries: Mutex<Vec<String>>,
    }

    impl SeqMock {
        fn new(fixtures: &[&str]) -> Self {
            Self {
                canned: Mutex::new(
                    fixtures
                        .iter()
                        .map(|f| SparqlResults::from_json(f).expect("valid fixture JSON"))
                        .collect(),
                ),
                queries: Mutex::new(Vec::new()),
            }
        }

        fn queries(&self) -> Vec<String> {
            self.queries.lock().expect("lock not poisoned").clone()
        }
    }

    #[async_trait]
    impl SparqlClient for SeqMock {
        async fn query(&self, sparql: &str) -> Result<SparqlResults, JoluxError> {
            self.queries
                .lock()
                .expect("lock not poisoned")
                .push(sparql.to_string());
            self.canned
                .lock()
                .expect("lock not poisoned")
                .pop_front()
                .ok_or_else(|| JoluxError::Transport("SeqMock: keine Fixture mehr".into()))
        }
    }

    const VERSION_FIX: &str = r#"{
      "head": {"vars": ["sub"]},
      "results": {"bindings": [
        {"sub":{"type":"uri","value":"https://fedlex.data.admin.ch/eli/cc/2017/762/text/20250101"}}
      ]}
    }"#;

    const OUT_FIX: &str = r#"{
      "head": {"vars": ["dst","desc"]},
      "results": {"bindings": [
        {"dst":{"type":"uri","value":"https://fedlex.data.admin.ch/eli/cc/1998/3033/text"},
         "desc":{"type":"literal","value":"Art. 31"}},
        {"dst":{"type":"uri","value":"https://fedlex.data.admin.ch/eli/cc/1998/3033/text"}},
        {"dst":{"type":"uri","value":"https://fedlex.data.admin.ch/eli/cc/1999/404/text"}}
      ]}
    }"#;

    const IN_FIX: &str = r#"{
      "head": {"vars": ["src","desc"]},
      "results": {"bindings": [
        {"src":{"type":"uri","value":"https://fedlex.data.admin.ch/eli/cc/2013/814/text/20251101"}},
        {"src":{"type":"uri","value":"https://fedlex.data.admin.ch/eli/cc/2013/814/text/20260201"}},
        {"src":{"type":"uri","value":"https://fedlex.data.admin.ch/eli/cc/2025/235/text/20260201"},
         "desc":{"type":"literal","value":"Art. 2"}}
      ]}
    }"#;

    const EMPTY_VERSION_FIX: &str = r#"{"head":{"vars":["sub"]},"results":{"bindings":[]}}"#;

    #[tokio::test]
    async fn outgoing_binds_resolved_version_and_deduplicates() {
        let client = SeqMock::new(&[VERSION_FIX, OUT_FIX]);
        let eli = Eli::new("eli/cc/2017/762").unwrap();
        let resp = get_citations(
            &client,
            &eli,
            CitationDirection::Outgoing,
            ValidAsOf::new(date!(2026 - 01 - 01)),
        )
        .await
        .unwrap();

        // Duplikat (J7.4) entfernt, Erlass-URIs statt /text-Formen.
        assert_eq!(resp.data().len(), 2, "war: {:?}", resp.data());
        assert_eq!(
            resp.data()[0].from,
            "https://fedlex.data.admin.ch/eli/cc/2017/762"
        );
        assert_eq!(
            resp.data()[0].to,
            "https://fedlex.data.admin.ch/eli/cc/1998/3033"
        );
        assert_eq!(resp.data()[0].description.as_deref(), Some("Art. 31"));

        let queries = client.queries();
        assert_eq!(queries.len(), 2);
        // Hauptquery: Stichtags-Auflösung, «from»-frei.
        assert!(queries[0].contains(r#"FILTER(?date <= xsd:date("2026-01-01"))"#));
        assert!(queries[0].contains("subdivision-type/text"));
        // Zweitquery: exakt an die Fassungs-URI gebunden, kein STRSTARTS-Scan.
        assert!(
            queries[1].contains("<https://fedlex.data.admin.ch/eli/cc/2017/762/text/20250101>")
        );
        assert!(!queries[1].contains("STRSTARTS"));
    }

    #[tokio::test]
    async fn outgoing_without_recorded_version_is_empty_not_error() {
        let client = SeqMock::new(&[EMPTY_VERSION_FIX]);
        let eli = Eli::new("eli/cc/2017/762").unwrap();
        let resp = get_citations(
            &client,
            &eli,
            CitationDirection::Outgoing,
            ValidAsOf::new(date!(1990 - 01 - 01)),
        )
        .await
        .unwrap();
        assert!(resp.data().is_empty());
        // Ohne aufgelöste Fassung darf keine Zitations-Query laufen.
        assert_eq!(client.queries().len(), 1);
    }

    #[tokio::test]
    async fn incoming_deduplicates_per_source_act() {
        let client = SeqMock::new(&[IN_FIX]);
        let eli = Eli::new("eli/cc/2022/491").unwrap();
        let resp = get_citations(
            &client,
            &eli,
            CitationDirection::Incoming,
            ValidAsOf::new(date!(2026 - 07 - 03)),
        )
        .await
        .unwrap();

        // Zwei Fassungen desselben Quellgesetzes -> ein Eintrag (J7.4).
        assert_eq!(resp.data().len(), 2, "war: {:?}", resp.data());
        assert_eq!(
            resp.data()[0].from,
            "https://fedlex.data.admin.ch/eli/cc/2013/814"
        );
        assert_eq!(
            resp.data()[0].to,
            "https://fedlex.data.admin.ch/eli/cc/2022/491"
        );
        assert_eq!(resp.data()[1].description.as_deref(), Some("Art. 2"));

        let q = &client.queries()[0];
        assert!(q.contains("<https://fedlex.data.admin.ch/eli/cc/2022/491/text>"));
        assert!(!q.contains("STRSTARTS"));
        assert!(!q.contains("UNION"));
    }

    #[tokio::test]
    async fn both_runs_two_short_queries_instead_of_union() {
        let client = SeqMock::new(&[VERSION_FIX, OUT_FIX, IN_FIX]);
        let eli = Eli::new("eli/cc/2017/762").unwrap();
        let resp = get_citations(
            &client,
            &eli,
            CitationDirection::Both,
            ValidAsOf::new(date!(2026 - 01 - 01)),
        )
        .await
        .unwrap();
        assert_eq!(resp.data().len(), 4);
        for q in client.queries() {
            assert!(!q.contains("UNION"), "UNION reisst die WAF-Schwelle: {q}");
        }
    }

    /// WAF-Betriebsregel (analog `waf_guard_main_queries_avoid_from` in
    /// `impacts.rs`): Die Hauptquery bleibt «from»-frei; die Zweitqueries
    /// tragen das Prädikat zwangsläufig und müssen dafür unter der
    /// empirischen Schwelle (~600 Zeichen inkl. Prefixes) bleiben.
    #[test]
    fn waf_guard_citation_queries() {
        assert!(
            !CIT_VERSION_Q.to_lowercase().contains("from"),
            "CIT_VERSION_Q muss «from»-frei bleiben (WAF), enthaelt: {CIT_VERSION_Q}"
        );
        for (name, q) in [("CIT_OUT_Q", CIT_OUT_Q), ("CIT_IN_Q", CIT_IN_Q)] {
            assert!(
                PREFIXES.len() + q.len() + 100 < 600,
                "{name} zu lang fuer die WAF-Schwelle: {} Zeichen",
                PREFIXES.len() + q.len()
            );
        }
    }

    #[test]
    fn act_uri_strips_text_suffixes() {
        assert_eq!(
            act_uri("https://fedlex.data.admin.ch/eli/cc/2013/814/text/20251101"),
            "https://fedlex.data.admin.ch/eli/cc/2013/814"
        );
        assert_eq!(
            act_uri("https://fedlex.data.admin.ch/eli/cc/1998/3033/text"),
            "https://fedlex.data.admin.ch/eli/cc/1998/3033"
        );
        assert_eq!(
            act_uri("https://fedlex.data.admin.ch/eli/cc/1998/3033"),
            "https://fedlex.data.admin.ch/eli/cc/1998/3033"
        );
    }
}
