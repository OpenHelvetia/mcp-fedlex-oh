//! Primitive: Stichtags-Auflösung der konsolidierten Fassung (Rulebook J2/J14).
//!
//! Der Kern der Bi-Temporalität: zu einem Stichtag die gültige Consolidation
//! (Versions-Fassung) finden und ihre XML-Manifestation liefern. Vermeidet den
//! Fehler „immer die neueste Fassung" (J3.1/J20.2).

use crate::client::{Language, PREFIXES, SparqlClient, val};
use crate::{eli_uri, error::JoluxError};
use fedlex_core::{Eli, Provenance, Response, TransactionTime, ValidAsOf};
use serde::{Deserialize, Serialize};

/// Eine zum Stichtag gültige konsolidierte Fassung eines Erlasses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Consolidation {
    /// URI des Consolidation-Knotens.
    pub consolidation_uri: String,
    /// Stand-Datum der Fassung (`jolux:dateApplicability`).
    pub date_applicability: String,
    /// Download-URL der AKN-XML-Manifestation.
    pub xml_url: String,
    /// Sprache der Manifestation.
    pub language: String,
}

// Verify-L5: `?repealed` (dateNoLongerInForce des Erlasses) fällt im SELBEN
// Round-Trip ab — kein zusätzlicher Query auf dem Hot-Read-Pfad. Damit weiss
// read_article/resolve_consolidation_at, ob der gelieferte Text die letzte
// Fassung eines aufgehobenen Erlasses ist.
const CONS_Q: &str = r#"SELECT ?cons ?date ?url ?repealed WHERE {
  OPTIONAL { <__URI__> jolux:dateNoLongerInForce ?repealed }
  ?cons jolux:isMemberOf <__URI__> ;
        jolux:dateApplicability ?date ;
        jolux:isRealizedBy ?expr .
  ?expr jolux:language <__LANGURI__> ;
        jolux:isEmbodiedBy ?manif .
  ?manif jolux:isExemplifiedBy ?url .
  FILTER(CONTAINS(STR(?url), "xml"))
  FILTER(?date <= xsd:date("__DATE__"))
} ORDER BY DESC(?date) LIMIT 1"#;
// Konstruktor- statt Literal-Form im Datumsvergleich: Virtuoso liefert mit
// `"…"^^xsd:date` bei einem Teil der Bestandsdaten still 0 Treffer
// (Betriebsregel Datumsvergleich, docs/dev/10_LEXICON_jolux.md; live-verifiziert
// 2026-07-05 an eli/cc/1959/1972_2034_2058).

/// Findet die zum Stichtag `as_of` gültige konsolidierte Fassung + ihre XML-URL.
///
/// Filtert über `dateApplicability <= as_of` und nimmt die jüngste (Rulebook
/// J14.3). Liefert [`JoluxError::NotFound`], wenn es zum Stichtag keine Fassung
/// gibt (z.B. vor Inkrafttreten).
pub async fn resolve_consolidation_at(
    client: &impl SparqlClient,
    eli: &Eli,
    as_of: ValidAsOf,
    lang: Language,
) -> Result<Response<Consolidation>, JoluxError> {
    let uri = eli_uri(eli);
    let sparql = format!(
        "{PREFIXES}{}",
        CONS_Q
            .replace("__URI__", &uri)
            .replace("__LANGURI__", lang.vocab_uri())
            .replace("__DATE__", &as_of.to_string())
    );
    let res = client.query(&sparql).await?;
    let b = res
        .bindings()
        .first()
        .ok_or_else(|| JoluxError::NotFound(uri.clone()))?;

    let cons = Consolidation {
        consolidation_uri: val(b, "cons").unwrap_or_default().to_string(),
        date_applicability: val(b, "date").unwrap_or_default().to_string(),
        xml_url: val(b, "url").unwrap_or_default().to_string(),
        language: lang.tag().to_string(),
    };

    // Die Provenance weist die tatsächlich aufgelöste Fassung aus — der
    // Stichtag allein suggeriert sonst eine Konsolidierung, die es (etwa bei
    // künftigen Stichtagen) nicht gibt.
    let mut prov = Provenance::new(eli.clone(), as_of, TransactionTime::now())
        .with_date_applicability(cons.date_applicability.clone());
    // Verify-L5: Nur flaggen, wenn der Erlass ZUM STICHTAG bereits aufgehoben
    // war (dateNoLongerInForce <= as_of). Ein künftiges Aufhebungsdatum lässt
    // den Erlass am Stichtag noch gelten — dann kein Warnsignal.
    if let Some(repealed) = val(b, "repealed").map(str::to_string) {
        // ISO-Datums-Strings vergleichen lexikografisch korrekt.
        if repealed.as_str() <= as_of.to_string().as_str() {
            prov = prov.with_repealed_since(repealed);
        }
    }
    Ok(Response::new(cons, prov))
}

/// Eine Fassung in der Versionsliste eines Erlasses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Version {
    /// URI des Consolidation-Knotens.
    pub consolidation_uri: String,
    /// Stand-Datum der Fassung.
    pub date_applicability: String,
}

// Verify-L11: Cap deutlich ueber das beobachtete Maximum (Bundesgesetz max
// 118 Fassungen, J14.1b) — damit ist die Liste fuer reale Erlasse immer
// vollstaendig; ein Agent, der 100 Eintraege sah, hatte keinen stillen Cap,
// sondern 100 echte Fassungen. Die Tool-Beschreibung sagt das jetzt auch.
const VERSIONS_Q: &str = r#"SELECT DISTINCT ?cons ?date WHERE {
  ?cons jolux:isMemberOf <__URI__> ;
        jolux:dateApplicability ?date .
} ORDER BY ?date LIMIT 500"#;

/// JLX-TMP-01: Listet alle Fassungen (Consolidations) eines Erlasses, chronologisch.
///
/// Versionsanzahl ist stark typabhängig (Bundesgesetz Ø 12.3, max 118, J14.1b).
/// 6'532 CAs haben gar keine Consolidations (J3.3) — dann leere Liste, kein Fehler.
pub async fn list_versions(
    client: &impl SparqlClient,
    eli: &Eli,
    as_of: ValidAsOf,
) -> Result<Response<Vec<Version>>, JoluxError> {
    let sparql = format!("{PREFIXES}{}", VERSIONS_Q.replace("__URI__", &eli_uri(eli)));
    let res = client.query(&sparql).await?;
    let versions = res
        .bindings()
        .iter()
        .filter_map(|b| {
            Some(Version {
                consolidation_uri: val(b, "cons")?.to_string(),
                date_applicability: val(b, "date")?.to_string(),
            })
        })
        .collect();
    let prov = Provenance::new(eli.clone(), as_of, TransactionTime::now());
    Ok(Response::new(versions, prov))
}

/// Ergebnis der Geltungsprüfung eines Erlasses.
///
/// **Zwei Zeitbezüge, bewusst getrennt benannt:** `in_force` beantwortet die
/// Frage zum **Stichtag** der Anfrage; `current_status_*` spiegelt den
/// **heutigen** Vokabular-Status des Erlasses bei Fedlex (der Graph kennt
/// keinen historisierten Status). Vor der Umbenennung standen beide Signale
/// unversöhnt nebeneinander — `in_force: false` (Stichtag vor Inkrafttreten)
/// neben `status_label: "In Kraft"` führte Konsumenten in die Irre.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InForce {
    /// Geltung zum Stichtag (Doppel-Logik, siehe [`check_in_force`]).
    pub in_force: bool,
    /// `jolux:inForceStatus` (opake Vokabular-URI), sofern vorhanden —
    /// **heutiger** Status, nicht stichtagsbezogen.
    #[serde(alias = "status_uri")]
    pub current_status_uri: Option<String>,
    /// Deutsches Label des **heutigen** Status (z.B. «In Kraft», «Nicht mehr
    /// in Kraft»), direkt in der Query gejoint (68 §C-1).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        alias = "status_label"
    )]
    pub current_status_label: Option<String>,
    /// Inkrafttreten.
    pub date_entry_in_force: Option<String>,
    /// Ausserkrafttreten (deckt 96 % der Abgelaufenen, J3.2).
    pub date_no_longer_in_force: Option<String>,
    /// Ende der Anwendbarkeit (Sonderfälle, 4 %).
    pub date_end_applicability: Option<String>,
    /// 68 §F-29: `true`, wenn der Stichtag NACH dem heutigen Schweizer
    /// Kalendertag liegt — die Aussage ist dann eine Projektion des heutigen
    /// Graphen (kuenftige Inkrafttretens-Daten sind bekannt, kuenftige
    /// Aufhebungen nicht zwingend), kein beglaubigter Zustand. Vorher wurde
    /// as_of=2999-12-31 kommentarlos als kind=norm beantwortet.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub future_as_of: bool,
    /// Verify-V3: `true`, wenn der Graph WEDER Status NOCH irgendein
    /// Geltungs-Datum zum Objekt kennt — `in_force: false` heisst dann
    /// «keine Daten», nicht «ausser Kraft». Vorher sah das Datenloch wie
    /// eine belastbare Negativ-Aussage aus (Stub `eli/cc/2020/2930_cc`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub no_enforcement_data: bool,
}

const IN_FORCE_Q: &str = r#"SELECT ?status ?statusLabel ?entry ?noLonger ?endApp WHERE {
  OPTIONAL { <__URI__> jolux:inForceStatus ?status
    OPTIONAL { ?status skos:prefLabel ?statusLabel . FILTER(LANG(?statusLabel) = "de") } }
  OPTIONAL { <__URI__> jolux:dateEntryInForce ?entry }
  OPTIONAL { <__URI__> jolux:dateNoLongerInForce ?noLonger }
  OPTIONAL { <__URI__> jolux:dateEndApplicability ?endApp }
} LIMIT 1"#;

/// JLX-TMP-03: Prüft, ob ein Erlass zum Stichtag gilt.
///
/// Zukunfts-Stichtage (68 §F-29): ein `as_of` nach dem heutigen Schweizer
/// Kalendertag wird beantwortet (Inkrafttretens-Daten koennen in der Zukunft
/// liegen), aber `future_as_of: true` kennzeichnet, dass die Aussage eine
/// Projektion des heutigen Graphen ist — kein beglaubigter Zustand.
///
/// Das Status-Feld allein genügt **nicht** — 15.1 % der CAs haben keinen
/// `inForceStatus`, 10'479 davon aber ein `dateEntryInForce` (J3.3). Deshalb
/// Doppel-Logik nach J3.2: primär über die Datumsfelder
/// (`entry <= as_of < min(noLonger, endApplicability)`), Fallback auf das
/// Status-Vokabular (`.../0` = in Kraft), wenn Daten fehlen.
pub async fn check_in_force(
    client: &impl SparqlClient,
    eli: &Eli,
    as_of: ValidAsOf,
) -> Result<Response<InForce>, JoluxError> {
    let uri = eli_uri(eli);
    let sparql = format!("{PREFIXES}{}", IN_FORCE_Q.replace("__URI__", &uri));
    let res = client.query(&sparql).await?;
    let b = res
        .bindings()
        .first()
        .ok_or_else(|| JoluxError::NotFound(uri.clone()))?;

    let current_status_uri = val(b, "status").map(str::to_string);
    let current_status_label = val(b, "statusLabel")
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let entry = val(b, "entry").map(str::to_string);
    let no_longer = val(b, "noLonger").map(str::to_string);
    let end_app = val(b, "endApp").map(str::to_string);

    // ISO-Datums-Strings vergleichen lexikografisch korrekt.
    let day = as_of.to_string();
    let started = entry.as_deref().is_some_and(|d| d <= day.as_str());
    let ended = [no_longer.as_deref(), end_app.as_deref()]
        .into_iter()
        .flatten()
        .any(|d| d <= day.as_str());
    let in_force = if entry.is_some() {
        started && !ended
    } else {
        // Fallback J3.3: kein Datum -> Status-Vokabular (0 = in Kraft).
        current_status_uri
            .as_deref()
            .is_some_and(|s| s.ends_with("/0"))
    };

    let no_enforcement_data =
        current_status_uri.is_none() && entry.is_none() && no_longer.is_none() && end_app.is_none();
    let data = InForce {
        in_force,
        no_enforcement_data,
        future_as_of: as_of.date() > fedlex_core::swiss_today(),
        current_status_uri,
        current_status_label,
        date_entry_in_force: entry,
        date_no_longer_in_force: no_longer,
        date_end_applicability: end_app,
    };
    let prov = Provenance::new(eli.clone(), as_of, TransactionTime::now());
    Ok(Response::new(data, prov))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::MockSparqlClient;
    use time::macros::date;

    /// Verify-V3: lauter Nulls hiess vorher stillschweigend in_force:false —
    /// ein Datenloch sah aus wie eine belastbare Negativ-Aussage.
    #[tokio::test]
    async fn empty_enforcement_data_is_flagged() {
        let empty = r#"{"head":{"vars":["status","statusLabel","entry","noLonger","endApp"]},"results":{"bindings":[{}]}}"#;
        let client = MockSparqlClient::from_json(empty);
        let eli = Eli::new("eli/cc/2020/2930_cc").unwrap();
        let resp = check_in_force(&client, &eli, ValidAsOf::new(date!(2026 - 07 - 07)))
            .await
            .unwrap();
        assert!(!resp.data().in_force);
        assert!(resp.data().no_enforcement_data, "{:?}", resp.data());
    }

    const FIXTURE: &str = r#"{
      "head": {"vars": ["cons","date","url"]},
      "results": {"bindings": [{
        "cons": {"type":"uri","value":"https://fedlex.data.admin.ch/eli/cc/2017/762/consolidation/20230601"},
        "date": {"type":"literal","value":"2023-06-01"},
        "url": {"type":"uri","value":"https://fedlex.data.admin.ch/eli/cc/2017/762/20230601/de/xml/fedlex-data-admin-ch-eli-cc-2017-762-20230601-de-xml.xml"}
      }]}
    }"#;

    /// Verify-L5: Ein aufgehobener Erlass, heute (nach Aufhebung) gelesen,
    /// traegt repealed_since — der letzte Text ist dann kein blanker Beleg.
    /// Ein KUENFTIGES Aufhebungsdatum flaggt NICHT (am Stichtag gilt er noch).
    #[tokio::test]
    async fn repealed_act_carries_repeal_date_only_when_past() {
        let fixture = |repealed: &str| {
            format!(
                r#"{{"head":{{"vars":["cons","date","url","repealed"]}},"results":{{"bindings":[{{
                  "cons":{{"type":"uri","value":"https://fedlex.data.admin.ch/eli/cc/1993/1945/consolidation/20190301"}},
                  "date":{{"type":"literal","value":"2019-03-01"}},
                  "url":{{"type":"uri","value":"https://fedlex.data.admin.ch/eli/cc/1993/1945/20190301/de/xml/x.xml"}},
                  "repealed":{{"type":"literal","value":"{repealed}"}}
                }}]}}}}"#
            )
        };
        let eli = Eli::new("eli/cc/1993/1945").unwrap();
        // Aufhebung 2023 liegt VOR dem Stichtag 2026 -> geflaggt.
        let past = MockSparqlClient::from_json(&fixture("2023-09-01"));
        let r = resolve_consolidation_at(
            &past,
            &eli,
            ValidAsOf::new(date!(2026 - 07 - 07)),
            Language::De,
        )
        .await
        .unwrap();
        assert_eq!(r.provenance().repealed_since.as_deref(), Some("2023-09-01"));
        // Derselbe Erlass an einem Stichtag VOR der Aufhebung -> kein Flag.
        let before = MockSparqlClient::from_json(&fixture("2023-09-01"));
        let r2 = resolve_consolidation_at(
            &before,
            &eli,
            ValidAsOf::new(date!(2020 - 01 - 01)),
            Language::De,
        )
        .await
        .unwrap();
        assert_eq!(r2.provenance().repealed_since, None);
        // Query holt das Aufhebungsdatum im selben Round-Trip.
        assert!(
            past.last_query()
                .unwrap()
                .contains("jolux:dateNoLongerInForce ?repealed")
        );
    }

    #[tokio::test]
    async fn resolves_version_at_stichtag_with_filter_and_language() {
        let client = MockSparqlClient::from_json(FIXTURE);
        let eli = Eli::new("eli/cc/2017/762").unwrap();
        let resp = resolve_consolidation_at(
            &client,
            &eli,
            ValidAsOf::new(date!(2023 - 06 - 15)),
            Language::De,
        )
        .await
        .unwrap();

        assert_eq!(resp.data().date_applicability, "2023-06-01");
        assert!(resp.data().xml_url.contains("/xml/"));
        assert_eq!(resp.data().language, "de");
        assert_eq!(resp.provenance().valid_as_of.to_string(), "2023-06-15");

        // Stichtags-Filter + Sprachvokabular landen korrekt in der Query.
        let q = client.last_query().unwrap();
        assert!(q.contains(r#"FILTER(?date <= xsd:date("2023-06-15"))"#));
        assert!(q.contains("/DEU"));
        assert!(q.contains("ORDER BY DESC(?date)"));
    }

    #[tokio::test]
    async fn no_version_before_entry_into_force_is_not_found() {
        let empty = r#"{"head":{"vars":["cons","date","url"]},"results":{"bindings":[]}}"#;
        let client = MockSparqlClient::from_json(empty);
        let eli = Eli::new("eli/cc/2017/762").unwrap();
        let err = resolve_consolidation_at(
            &client,
            &eli,
            ValidAsOf::new(date!(1990 - 01 - 01)),
            Language::De,
        )
        .await
        .unwrap_err();
        assert!(matches!(err, JoluxError::NotFound(_)));
    }

    #[tokio::test]
    async fn versions_are_listed_chronologically() {
        let client = MockSparqlClient::from_json(
            r#"{"head":{"vars":["cons","date"]},"results":{"bindings":[
              {"cons":{"type":"uri","value":"https://fedlex.data.admin.ch/eli/cc/2017/762/20180101"},
               "date":{"type":"literal","value":"2018-01-01"}},
              {"cons":{"type":"uri","value":"https://fedlex.data.admin.ch/eli/cc/2017/762/20230601"},
               "date":{"type":"literal","value":"2023-06-01"}}
            ]}}"#,
        );
        let eli = Eli::new("eli/cc/2017/762").unwrap();
        let resp = list_versions(&client, &eli, ValidAsOf::new(date!(2026 - 01 - 01)))
            .await
            .unwrap();
        assert_eq!(resp.data().len(), 2);
        assert_eq!(resp.data()[0].date_applicability, "2018-01-01");

        let q = client.last_query().unwrap();
        assert!(q.contains("jolux:isMemberOf"));
        assert!(q.contains("ORDER BY ?date"));
    }

    #[tokio::test]
    async fn in_force_uses_date_double_logic_not_just_status() {
        // Status sagt "in Kraft" (/0), aber dateNoLongerInForce liegt vor dem
        // Stichtag -> Datumslogik gewinnt (J3.2).
        let client = MockSparqlClient::from_json(
            r#"{"head":{"vars":["status","entry","noLonger","endApp"]},"results":{"bindings":[{
              "status":{"type":"uri","value":"https://fedlex.data.admin.ch/vocabulary/enforcement-status/0"},
              "entry":{"type":"literal","value":"2000-01-01"},
              "noLonger":{"type":"literal","value":"2010-01-01"}
            }]}}"#,
        );
        let eli = Eli::new("eli/cc/1999/27").unwrap();
        let resp = check_in_force(&client, &eli, ValidAsOf::new(date!(2020 - 01 - 01)))
            .await
            .unwrap();
        assert!(!resp.data().in_force, "Datumslogik muss Status übersteuern");
        assert_eq!(
            resp.data().date_no_longer_in_force.as_deref(),
            Some("2010-01-01")
        );
    }

    #[tokio::test]
    async fn in_force_falls_back_to_status_without_dates() {
        let client = MockSparqlClient::from_json(
            r#"{"head":{"vars":["status","statusLabel","entry","noLonger","endApp"]},"results":{"bindings":[{
              "status":{"type":"uri","value":"https://fedlex.data.admin.ch/vocabulary/enforcement-status/0"},
              "statusLabel":{"type":"literal","xml:lang":"de","value":"In Kraft"}
            }]}}"#,
        );
        let eli = Eli::new("eli/cc/2017/762").unwrap();
        let resp = check_in_force(&client, &eli, ValidAsOf::new(date!(2026 - 01 - 01)))
            .await
            .unwrap();
        assert!(resp.data().in_force, "Fallback auf Status-Vokabular (J3.3)");
        // 68 §C-1: Label direkt aus der Query — kein resolve_vocabulary_label nötig.
        assert_eq!(
            resp.data().current_status_label.as_deref(),
            Some("In Kraft")
        );
    }
}
