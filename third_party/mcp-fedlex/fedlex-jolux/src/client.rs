//! SPARQL-Client-Abstraktion + Helfer.
//!
//! Primitive sprechen ausschliesslich gegen das [`SparqlClient`]-Trait. Dadurch
//! sind sie **deterministisch mit Fixtures** ([`MockSparqlClient`]) testbar und
//! später **live** gegen `fedlex.data.admin.ch` betreibbar — ohne Änderung an den
//! Primitiven selbst.

use crate::error::JoluxError;
use async_trait::async_trait;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

/// Vorangestellte PREFIX-Deklarationen für alle JOLux-Queries (Rulebook J0.2).
pub const PREFIXES: &str = "PREFIX jolux: <http://data.legilux.public.lu/resource/ontology/jolux#>\n\
PREFIX skos: <http://www.w3.org/2004/02/skos/core#>\n\
PREFIX xsd: <http://www.w3.org/2001/XMLSchema#>\n";

/// Abstraktion über einen SPARQL-Endpoint.
#[async_trait]
pub trait SparqlClient: Send + Sync {
    /// Führt eine SPARQL-SELECT-Query aus und liefert die JSON-Ergebnisse.
    async fn query(&self, sparql: &str) -> Result<SparqlResults, JoluxError>;
}

/// SPARQL-1.1-JSON-Ergebnisse (die Teilmenge, die wir auswerten).
#[derive(Debug, Clone, Deserialize)]
pub struct SparqlResults {
    /// Kopf mit den Variablennamen.
    pub head: SparqlHead,
    /// Ergebnis-Bindings.
    pub results: SparqlBindingsBlock,
}

/// Kopf-Block (Variablennamen).
#[derive(Debug, Clone, Deserialize)]
pub struct SparqlHead {
    /// Die im SELECT projizierten Variablen.
    #[serde(default)]
    pub vars: Vec<String>,
}

/// Ergebnis-Block mit den Bindings.
#[derive(Debug, Clone, Deserialize)]
pub struct SparqlBindingsBlock {
    /// Eine Zeile pro Treffer: Variable -> Wert.
    #[serde(default)]
    pub bindings: Vec<Binding>,
}

/// Ein Binding: Variablenname -> Wert.
pub type Binding = BTreeMap<String, SparqlValue>;

/// Ein einzelner SPARQL-Term (uri | literal | bnode).
#[derive(Debug, Clone, Deserialize)]
pub struct SparqlValue {
    /// Art des Terms (`uri`, `literal`, `bnode`).
    #[serde(rename = "type")]
    pub kind: String,
    /// Der Wert als Zeichenkette.
    pub value: String,
    /// Sprach-Tag eines Literals (`xml:lang`), falls vorhanden.
    #[serde(rename = "xml:lang", default)]
    pub lang: Option<String>,
    /// Datentyp eines typisierten Literals, falls vorhanden.
    #[serde(default)]
    pub datatype: Option<String>,
}

impl SparqlResults {
    /// Parst SPARQL-Ergebnisse aus einem JSON-String (für Fixtures/Tests).
    pub fn from_json(s: &str) -> Result<Self, JoluxError> {
        serde_json::from_str(s).map_err(|e| JoluxError::MalformedResults(e.to_string()))
    }

    /// Die Ergebnis-Bindings (eine Zeile pro Treffer).
    pub fn bindings(&self) -> &[Binding] {
        &self.results.bindings
    }

    /// Ob kein Treffer vorliegt.
    pub fn is_empty(&self) -> bool {
        self.results.bindings.is_empty()
    }
}

/// Liest den Wert einer Variable aus einem Binding.
pub fn val<'a>(binding: &'a Binding, var: &str) -> Option<&'a str> {
    binding.get(var).map(|v| v.value.as_str())
}

/// Deterministischer Mock-Client für Tests. Liefert für jede Query dasselbe
/// vorbereitete Ergebnis und merkt sich die zuletzt gestellte Query.
#[derive(Debug, Clone)]
pub struct MockSparqlClient {
    canned: SparqlResults,
    /// Vorab eingereihte Antworten (FIFO); ist die Queue leer, antwortet
    /// `canned`. Für Primitive, die mehrere Queries je Aufruf stellen
    /// (z. B. `search_law` mit Kürzel-Vorabfrage).
    queued: Arc<Mutex<std::collections::VecDeque<SparqlResults>>>,
    queries: Arc<Mutex<Vec<String>>>,
    /// Verify-L7: erzwingt einen Fehler statt einer Antwort (Fehlerpfad-Tests).
    error: Option<JoluxError>,
}

impl MockSparqlClient {
    /// Erzeugt einen Mock über einem festen Ergebnis.
    pub fn new(canned: SparqlResults) -> Self {
        Self {
            canned,
            queued: Arc::new(Mutex::new(std::collections::VecDeque::new())),
            queries: Arc::new(Mutex::new(Vec::new())),
            error: None,
        }
    }

    /// Erzeugt einen Mock, der jede Query mit dem gegebenen Fehler abweist
    /// (Verify-L7: Fehlerklassifikation testen).
    pub fn from_error(error: JoluxError) -> Self {
        let mut mock = Self::new(
            SparqlResults::from_json(r#"{"head":{"vars":[]},"results":{"bindings":[]}}"#)
                .expect("leeres Fixture"),
        );
        mock.error = Some(error);
        mock
    }

    /// Erzeugt einen Mock aus einer JSON-Fixture.
    pub fn from_json(json: &str) -> Self {
        Self::new(SparqlResults::from_json(json).expect("valid fixture JSON"))
    }

    /// Erzeugt einen Mock, der die Fixtures der Reihe nach liefert und
    /// danach auf die letzte zurückfällt.
    pub fn from_json_sequence(fixtures: &[&str]) -> Self {
        assert!(!fixtures.is_empty(), "mindestens eine Fixture");
        let mut parsed: std::collections::VecDeque<SparqlResults> = fixtures
            .iter()
            .map(|j| SparqlResults::from_json(j).expect("valid fixture JSON"))
            .collect();
        let canned = parsed.pop_back().expect("nicht leer");
        let mock = Self::new(canned);
        *mock.queued.lock().expect("lock not poisoned") = parsed;
        mock
    }

    /// Die zuletzt an [`query`](SparqlClient::query) übergebene SPARQL-Query.
    pub fn last_query(&self) -> Option<String> {
        self.queries
            .lock()
            .expect("lock not poisoned")
            .last()
            .cloned()
    }

    /// Alle bisher gestellten Queries, in Reihenfolge.
    pub fn queries(&self) -> Vec<String> {
        self.queries.lock().expect("lock not poisoned").clone()
    }
}

#[async_trait]
impl SparqlClient for MockSparqlClient {
    async fn query(&self, sparql: &str) -> Result<SparqlResults, JoluxError> {
        self.queries
            .lock()
            .expect("lock not poisoned")
            .push(sparql.to_string());
        if let Some(err) = &self.error {
            return Err(err.clone());
        }
        if let Some(next) = self.queued.lock().expect("lock not poisoned").pop_front() {
            return Ok(next);
        }
        Ok(self.canned.clone())
    }
}

/// Amtssprachen und ihre EU-Sprachvokabular-URIs (Rulebook J0.2/J17.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    /// Deutsch.
    De,
    /// Französisch.
    Fr,
    /// Italienisch.
    It,
    /// Englisch.
    En,
    /// Rätoromanisch.
    Roh,
}

impl Language {
    /// EU-Sprachvokabular-URI (für `jolux:language`).
    pub fn vocab_uri(self) -> &'static str {
        match self {
            Language::De => "http://publications.europa.eu/resource/authority/language/DEU",
            Language::Fr => "http://publications.europa.eu/resource/authority/language/FRA",
            Language::It => "http://publications.europa.eu/resource/authority/language/ITA",
            Language::En => "http://publications.europa.eu/resource/authority/language/ENG",
            Language::Roh => "http://publications.europa.eu/resource/authority/language/ROH",
        }
    }

    /// Kurz-Tag (`xml:lang`-Wert) der Sprache.
    pub fn tag(self) -> &'static str {
        match self {
            Language::De => "de",
            Language::Fr => "fr",
            Language::It => "it",
            Language::En => "en",
            Language::Roh => "rm",
        }
    }

    /// Umkehrung von [`Language::vocab_uri`] (68 §C-1): mappt eine
    /// EU-Sprachvokabular-URI zurück auf die Sprache. `None` für fremde URIs.
    pub fn from_vocab_uri(uri: &str) -> Option<Self> {
        [
            Language::De,
            Language::Fr,
            Language::It,
            Language::En,
            Language::Roh,
        ]
        .into_iter()
        .find(|l| l.vocab_uri() == uri)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mock_returns_canned_and_records_query() {
        let client = MockSparqlClient::from_json(
            r#"{"head":{"vars":["x"]},"results":{"bindings":[{"x":{"type":"literal","value":"42"}}]}}"#,
        );
        let res = client.query("SELECT ?x WHERE { ?x ?p ?o }").await.unwrap();
        assert_eq!(val(&res.bindings()[0], "x"), Some("42"));
        assert!(client.last_query().unwrap().contains("SELECT ?x"));
    }

    #[test]
    fn language_uris_and_tags() {
        assert!(Language::De.vocab_uri().ends_with("/DEU"));
        assert_eq!(Language::Roh.tag(), "rm");
    }
}
