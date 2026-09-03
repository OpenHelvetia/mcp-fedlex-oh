//! Primitive: Gesetzes-Metadaten (Rulebook J1/J3).

use crate::client::{PREFIXES, SparqlClient, val};
use crate::{eli_uri, error::JoluxError};
use fedlex_core::{Eli, Provenance, Response, TransactionTime, ValidAsOf};
use serde::{Deserialize, Serialize};

/// Kern-Metadaten eines Erlasses auf ConsolidationAbstract-Ebene.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LawMetadata {
    /// ELI des Erlasses.
    pub eli: String,
    /// SR-Nummer: primär `jolux:historicalLegalId`; fehlt das Literal (neue
    /// Konsolidierungen wie das nDSG), fällt die Query auf die Systematik-
    /// Taxonomie zurück (`skos:notation`, 68 §F-11). Für EINEN bekannten
    /// Erlass ist der Join billig — anders als in der Breitensuche.
    pub sr_number: Option<String>,
    /// Volltitel (Expression-Ebene, deutsch).
    pub title: Option<String>,
    /// Amtliche Abkürzung (`jolux:titleShort`, z.B. «DSG»), sofern vorhanden
    /// (68 §F-11 — fürs Zitieren neben der SR-Nummer das wichtigste Kurzfeld).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub abbreviation: Option<String>,
    /// Erlass-Datum (`jolux:dateDocument`).
    pub date_document: Option<String>,
    /// Inkrafttreten (`jolux:dateEntryInForce`).
    pub date_entry_in_force: Option<String>,
    /// Heutiger Geltungsstatus (opake Vokabular-URI, `jolux:inForceStatus`).
    /// Namensgleich mit `check_in_force`: der Status beschreibt HEUTE, nicht
    /// den Stichtag — stichtagsgenau prüft `check_in_force`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_status_uri: Option<String>,
    /// Deutsches Label zum Status (z.B. «in Kraft»), direkt gejoint (68 §C-1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_status_label: Option<String>,
    /// Dokumenttyp-URI (`jolux:typeDocument`, opak -> via Vocabulary auflösen).
    pub type_document: Option<String>,
    /// Deutsches Label des Dokumenttyps (z.B. «Bundesgesetz»), direkt in
    /// der Query gejoint (68 §C-1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub type_document_label: Option<String>,
}

// Das CA-Typ-Pattern ist PFLICHT (68 §F-10): vorher war alles OPTIONAL, ein
// syntaktisch valider, nicht existierender ELI ergab ein Null-Objekt mit
// kind=norm — ein «Beleg» über Nichts. Jetzt: 0 Bindings -> NotFound.
const META_Q: &str = r#"SELECT ?sr ?srTax ?title ?short ?dateDocument ?dateEntryInForce ?status ?statusLabel ?typeDocument ?typeLabel WHERE {
  <__URI__> a jolux:ConsolidationAbstract .
  OPTIONAL { <__URI__> jolux:historicalLegalId ?sr }
  OPTIONAL { <__URI__> jolux:classifiedByTaxonomyEntry ?tax .
    ?tax skos:notation ?srTax .
    FILTER(DATATYPE(?srTax) = <https://fedlex.data.admin.ch/vocabulary/notation-type/id-systematique>) }
  OPTIONAL { <__URI__> jolux:dateDocument ?dateDocument }
  OPTIONAL { <__URI__> jolux:dateEntryInForce ?dateEntryInForce }
  OPTIONAL { <__URI__> jolux:inForceStatus ?status
    OPTIONAL { ?status skos:prefLabel ?statusLabel . FILTER(LANG(?statusLabel) = "de") } }
  OPTIONAL { <__URI__> jolux:typeDocument ?typeDocument
    OPTIONAL { ?typeDocument skos:prefLabel ?typeLabel . FILTER(LANG(?typeLabel) = "de") } }
  OPTIONAL {
    <__URI__> jolux:isRealizedBy ?expr .
    ?expr jolux:language <http://publications.europa.eu/resource/authority/language/DEU> ;
          jolux:title ?title .
    OPTIONAL { ?expr jolux:titleShort ?short }
  }
} LIMIT 1"#;

/// Holt die Kern-Metadaten eines Erlasses.
///
/// Liefert eine [`Response`] mit Provenance (ELI + `as_of`). Felder sind
/// `Option`, weil auf CA-Ebene mehrere Prädikate systematisch leer sein können
/// (Rulebook J1.2/J3.4) — `OPTIONAL` ist daher Pflicht. Existiert der ELI
/// nicht als ConsolidationAbstract, kommt [`JoluxError::NotFound`] statt eines
/// Null-Objekts mit Norm-Provenance (68 §F-10).
///
/// **Live-verifiziert (2026-06-10):** Titel über die CA-direkte Expression
/// (`<CA> isRealizedBy`), nicht über Consolidation-Expressions (die tragen nur
/// technische Labels).
pub async fn get_law_metadata(
    client: &impl SparqlClient,
    eli: &Eli,
    as_of: ValidAsOf,
) -> Result<Response<LawMetadata>, JoluxError> {
    let uri = eli_uri(eli);
    let sparql = format!("{PREFIXES}{}", META_Q.replace("__URI__", &uri));
    let res = client.query(&sparql).await?;
    let b = res
        .bindings()
        .first()
        .ok_or_else(|| JoluxError::NotFound(uri.clone()))?;

    let meta = LawMetadata {
        eli: eli.as_str().to_string(),
        // 68 §F-11: historicalLegalId zuerst, Taxonomie-Notation als Fallback.
        sr_number: val(b, "sr").or_else(|| val(b, "srTax")).map(str::to_string),
        title: val(b, "title").map(str::to_string),
        abbreviation: val(b, "short")
            .filter(|s| !s.is_empty())
            .map(str::to_string),
        date_document: val(b, "dateDocument").map(str::to_string),
        date_entry_in_force: val(b, "dateEntryInForce").map(str::to_string),
        current_status_uri: val(b, "status").map(str::to_string),
        current_status_label: val(b, "statusLabel")
            .filter(|s| !s.is_empty())
            .map(str::to_string),
        type_document: val(b, "typeDocument").map(str::to_string),
        type_document_label: val(b, "typeLabel")
            .filter(|s| !s.is_empty())
            .map(str::to_string),
    };

    let prov = Provenance::new(eli.clone(), as_of, TransactionTime::now());
    Ok(Response::new(meta, prov))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::MockSparqlClient;
    use time::macros::date;

    const FIXTURE: &str = r#"{
      "head": {"vars": ["sr","title","dateDocument","dateEntryInForce","typeDocument","typeLabel"]},
      "results": {"bindings": [{
        "sr": {"type":"literal","value":"730.0"},
        "title": {"type":"literal","xml:lang":"de","value":"Energiegesetz vom 30. September 2016 (EnG)"},
        "dateDocument": {"type":"literal","value":"2016-09-30"},
        "dateEntryInForce": {"type":"literal","value":"2018-01-01"},
        "typeDocument": {"type":"uri","value":"https://fedlex.data.admin.ch/vocabulary/resource-type/21"},
        "typeLabel": {"type":"literal","xml:lang":"de","value":"Bundesgesetz"}
      }]}
    }"#;

    #[tokio::test]
    async fn parses_metadata_and_carries_provenance() {
        let client = MockSparqlClient::from_json(FIXTURE);
        let eli = Eli::new("eli/cc/2017/762").unwrap();
        let resp = get_law_metadata(&client, &eli, ValidAsOf::new(date!(2024 - 01 - 01)))
            .await
            .unwrap();

        assert_eq!(resp.data().sr_number.as_deref(), Some("730.0"));
        assert!(
            resp.data()
                .title
                .as_deref()
                .unwrap()
                .contains("Energiegesetz")
        );
        assert_eq!(
            resp.data().date_entry_in_force.as_deref(),
            Some("2018-01-01")
        );
        // 68 §C-1: Dokumenttyp-Label direkt gejoint.
        assert_eq!(
            resp.data().type_document_label.as_deref(),
            Some("Bundesgesetz")
        );

        // ADR-004: Provenance trägt ELI + Stichtag.
        assert_eq!(resp.provenance().eli.as_str(), "eli/cc/2017/762");
        assert_eq!(resp.provenance().valid_as_of.to_string(), "2024-01-01");

        // Die Query expandiert den ELI zur vollen Fedlex-URI.
        let q = client.last_query().unwrap();
        assert!(q.contains("<https://fedlex.data.admin.ch/eli/cc/2017/762>"));
        assert!(q.contains("jolux:historicalLegalId"));
    }

    /// 68 §F-10 (Explorer-Befund, invertiert das fruehere Verhalten): ein
    /// nicht existierender ELI lieferte ein Null-Objekt mit kind=norm — ein
    /// «Beleg» ueber Nichts. Jetzt: NotFound.
    #[tokio::test]
    async fn nonexistent_eli_is_not_found_not_null_norm() {
        let empty = r#"{"head":{"vars":[]},"results":{"bindings":[]}}"#;
        let client = MockSparqlClient::from_json(empty);
        let eli = Eli::new("eli/cc/9999/99999").unwrap();
        let err = get_law_metadata(&client, &eli, ValidAsOf::new(date!(2024 - 01 - 01)))
            .await
            .unwrap_err();
        assert!(matches!(err, JoluxError::NotFound(_)));
        // Die Existenz haengt am Pflicht-Pattern des CA-Typs.
        assert!(
            client
                .last_query()
                .unwrap()
                .contains("a jolux:ConsolidationAbstract .")
        );
    }

    /// 68 §F-11: SR-Nummer faellt auf die Taxonomie-Notation zurueck (nDSG
    /// traegt kein historicalLegalId), Abkuerzung und Status kommen mit.
    #[tokio::test]
    async fn sr_fallback_abbreviation_and_status_are_carried() {
        let ndsg = r#"{
          "head": {"vars": ["sr","srTax","title","short","status","statusLabel"]},
          "results": {"bindings": [{
            "srTax": {"type":"literal","value":"235.1"},
            "title": {"type":"literal","xml:lang":"de","value":"Bundesgesetz vom 25. September 2020 ueber den Datenschutz"},
            "short": {"type":"literal","value":"DSG"},
            "status": {"type":"uri","value":"https://fedlex.data.admin.ch/vocabulary/enforcement-status/0"},
            "statusLabel": {"type":"literal","xml:lang":"de","value":"in Kraft"}
          }]}
        }"#;
        let client = MockSparqlClient::from_json(ndsg);
        let eli = Eli::new("eli/cc/2022/491").unwrap();
        let resp = get_law_metadata(&client, &eli, ValidAsOf::new(date!(2026 - 07 - 06)))
            .await
            .unwrap();
        assert_eq!(resp.data().sr_number.as_deref(), Some("235.1"));
        assert_eq!(resp.data().abbreviation.as_deref(), Some("DSG"));
        assert_eq!(
            resp.data().current_status_label.as_deref(),
            Some("in Kraft")
        );
        assert!(
            resp.data()
                .current_status_uri
                .as_deref()
                .unwrap()
                .ends_with("/0")
        );
    }
}
