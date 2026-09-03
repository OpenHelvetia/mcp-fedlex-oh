//! Primitive: Völkerrecht — Staatsverträge (Lexikon JLX-TRT-01/02,
//! Rulebook J12).

use crate::client::{Language, PREFIXES, SparqlClient, val};
use crate::error::JoluxError;
use serde::{Deserialize, Serialize};

/// Wählt aus sprach-getaggten Titeln den besten (68 §C-4): erst die
/// gewünschte Sprache, dann Amtssprachen-Fallback, dann irgendein
/// nicht-leerer. Fedlex trägt an Vertragsprozessen teils **leere**
/// Sprachvarianten (live: `titleTreaty@en = ""`) — Leerstrings sind
/// vorgefiltert, sonst gewinnt zufällig die leere Zeile.
fn pick_title(titles: &[(String, String)], lang: Language) -> Option<String> {
    let prefs = [lang.tag(), "de", "fr", "it", "en", "rm"];
    for pref in prefs {
        if let Some((_, t)) = titles.iter().find(|(l, _)| l == pref) {
            return Some(t.clone());
        }
    }
    titles.first().map(|(_, t)| t.clone())
}

/// Informationen zu einem Vertragsprozess (`jolux:TreatyProcess`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TreatyInfo {
    /// URI des TreatyProcess (`eli/treaty/YYYY/NNNN`).
    pub process_uri: String,
    /// Vertragstitel, sofern vorhanden.
    pub title: Option<String>,
    /// Unterzeichnungsdatum.
    pub signature_date: Option<String>,
    /// Unterzeichnungsort.
    pub signature_place: Option<String>,
    /// Bilateral-Flag (1.6 % der Prozesse ohne Flag, J12.2).
    pub bilateral: Option<bool>,
    /// Vertragsparteien (Länder-Vokabular-URIs).
    pub party_countries: Vec<String>,
    /// Genehmigungs-Bundesbeschluss (`approbationAct`), sofern vorhanden.
    pub approbation_act: Option<String>,
}

const TREATY_Q: &str = r#"SELECT ?title ?titleLang ?sigDate ?sigPlace ?bilateral ?country ?approbation WHERE {
  OPTIONAL { <__URI__> jolux:titleTreaty ?title . BIND(LANG(?title) AS ?titleLang) FILTER(STR(?title) != "") }
  OPTIONAL { <__URI__> jolux:treatySignatureDate ?sigDate }
  OPTIONAL { <__URI__> jolux:treatySignaturePlace ?sigPlace }
  OPTIONAL { <__URI__> jolux:bilateral ?bilateral }
  OPTIONAL { <__URI__> jolux:treatyPartyCountry ?country }
  OPTIONAL { <__URI__> jolux:approbationAct ?approbation }
} LIMIT 100"#;

/// JLX-TRT-01: Steckbrief eines Staatsvertrags-Prozesses.
///
/// Einstieg über die TreatyProcess-URI (`eli/treaty/...`). Der
/// `approbation_act` verknüpft zum Genehmigungs-Bundesbeschluss — eine
/// eigene Erlass-Kette (J12). Der Titel wird sprach-präferent gewählt
/// (68 §C-4, [`pick_title`]). Liefert [`JoluxError::NotFound`], wenn der
/// Knoten keine Treaty-Prädikate trägt.
pub async fn get_treaty_info(
    client: &impl SparqlClient,
    process_uri: &str,
    lang: Language,
) -> Result<TreatyInfo, JoluxError> {
    let safe = process_uri.replace(['<', '>', '"', '\\', ' '], "");
    let sparql = format!("{PREFIXES}{}", TREATY_Q.replace("__URI__", &safe));
    let res = client.query(&sparql).await?;
    if res.is_empty() {
        return Err(JoluxError::NotFound(safe));
    }

    let first = &res.bindings()[0];
    let mut info = TreatyInfo {
        process_uri: safe,
        title: None,
        signature_date: val(first, "sigDate").map(str::to_string),
        signature_place: val(first, "sigPlace").map(str::to_string),
        bilateral: val(first, "bilateral").map(|b| b == "true" || b == "1"),
        party_countries: Vec::new(),
        approbation_act: val(first, "approbation").map(str::to_string),
    };
    let mut titles: Vec<(String, String)> = Vec::new();
    for b in res.bindings() {
        if let Some(c) = val(b, "country")
            && !info.party_countries.iter().any(|x| x == c)
        {
            info.party_countries.push(c.to_string());
        }
        if let Some(t) = val(b, "title")
            && !t.is_empty()
        {
            let tl = val(b, "titleLang").unwrap_or_default().to_string();
            if !titles.iter().any(|(l, v)| *l == tl && v == t) {
                titles.push((tl, t.to_string()));
            }
        }
    }
    info.title = pick_title(&titles, lang);
    Ok(info)
}

/// Ein Treffer der Vertrags-Suche.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TreatyHit {
    /// URI des TreatyProcess.
    pub process_uri: String,
    /// Vertragstitel in der angefragten Sprache, sofern vorhanden (68 §C-4).
    pub title: Option<String>,
    /// Unterzeichnungsdatum, sofern vorhanden.
    pub signature_date: Option<String>,
}

// Titel direkt im Treffer (68 §C-4): ohne ihn weiss der Agent nur „Prozess-URI
// + Datum" und muss pro Treffer ein get_treaty_info nachschieben (N+1-Calls).
// Sprach-Filter serverseitig; leere Literale (live beobachtet) ausgeschlossen.
const FIND_TREATIES_Q: &str = r#"SELECT DISTINCT ?process ?title ?sigDate WHERE {
  ?process a jolux:TreatyProcess .
__FILTERS__  OPTIONAL { ?process jolux:titleTreaty ?title . FILTER(LANG(?title) = "__LANG__" && STR(?title) != "") }
  OPTIONAL { ?process jolux:treatySignatureDate ?sigDate }
} ORDER BY DESC(?sigDate) LIMIT __LIMIT__"#;

/// JLX-TRT-02: Findet Vertragsprozesse nach Land und/oder Bilateral-Flag.
///
/// Enumerate über `treatyPartyCountry` (Vokabular `country`, 429 Einträge)
/// und `bilateral` (J12.2/J12.3). Mindestens ein Filter ist sinnvoll —
/// ohne Filter werden schlicht die jüngsten Prozesse geliefert. Der Titel
/// kommt in der angefragten Sprache mit; fehlt die Sprachvariante, bleibt
/// er `None` (dann `get_treaty_info` fragen, das über Sprachen fällt).
pub async fn find_treaties(
    client: &impl SparqlClient,
    country_uri: Option<&str>,
    bilateral: Option<bool>,
    limit: u32,
    lang: Language,
) -> Result<Vec<TreatyHit>, JoluxError> {
    let mut filters = String::new();
    if let Some(c) = country_uri {
        let safe = c.replace(['<', '>', '"', '\\', ' '], "");
        filters.push_str(&format!("  ?process jolux:treatyPartyCountry <{safe}> .\n"));
    }
    if let Some(b) = bilateral {
        filters.push_str(&format!("  ?process jolux:bilateral {b} .\n"));
    }
    let sparql = format!(
        "{PREFIXES}{}",
        FIND_TREATIES_Q
            .replace("__FILTERS__", &filters)
            .replace("__LANG__", lang.tag())
            .replace("__LIMIT__", &limit.to_string())
    );
    let res = client.query(&sparql).await?;
    Ok(res
        .bindings()
        .iter()
        .filter_map(|b| {
            Some(TreatyHit {
                process_uri: val(b, "process")?.to_string(),
                title: val(b, "title")
                    .filter(|t| !t.is_empty())
                    .map(str::to_string),
                signature_date: val(b, "sigDate").map(str::to_string),
            })
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::MockSparqlClient;

    #[tokio::test]
    async fn treaty_info_aggregates_countries() {
        let client = MockSparqlClient::from_json(
            r#"{"head":{"vars":["title","titleLang","sigDate","sigPlace","bilateral","country","approbation"]},
            "results":{"bindings":[
              {"title":{"type":"literal","xml:lang":"de","value":"Abkommen X"},
               "titleLang":{"type":"literal","value":"de"},
               "sigDate":{"type":"literal","value":"1999-06-21"},
               "bilateral":{"type":"literal","value":"true"},
               "country":{"type":"uri","value":"https://fedlex.data.admin.ch/vocabulary/country/136"}},
              {"title":{"type":"literal","xml:lang":"de","value":"Abkommen X"},
               "titleLang":{"type":"literal","value":"de"},
               "sigDate":{"type":"literal","value":"1999-06-21"},
               "bilateral":{"type":"literal","value":"true"},
               "country":{"type":"uri","value":"https://fedlex.data.admin.ch/vocabulary/country/336"}}
            ]}}"#,
        );
        let info = get_treaty_info(
            &client,
            "https://fedlex.data.admin.ch/eli/treaty/1999/0001",
            Language::De,
        )
        .await
        .unwrap();
        assert_eq!(info.bilateral, Some(true));
        assert_eq!(info.party_countries.len(), 2);
        assert_eq!(info.signature_date.as_deref(), Some("1999-06-21"));
        assert_eq!(info.title.as_deref(), Some("Abkommen X"));
    }

    /// 68 §C-4: Fedlex trägt an Vertragsprozessen leere Sprachvarianten
    /// (live: `titleTreaty@en = ""` am CH–DE-Vertrag 2024/0088). Die leere
    /// Zeile darf nie gewinnen; fehlt die Wunschsprache, fällt die Wahl auf
    /// die nächste Amtssprache zurück.
    #[tokio::test]
    async fn empty_title_variant_never_wins_and_fallback_applies() {
        let client = MockSparqlClient::from_json(
            r#"{"head":{"vars":["title","titleLang"]},
            "results":{"bindings":[
              {"title":{"type":"literal","xml:lang":"en","value":""},
               "titleLang":{"type":"literal","value":"en"}},
              {"title":{"type":"literal","xml:lang":"fr","value":"Convention d'application"},
               "titleLang":{"type":"literal","value":"fr"}}
            ]}}"#,
        );
        // Wunschsprache de fehlt → fr ist die nächste Präferenz, nie das leere en.
        let info = get_treaty_info(
            &client,
            "https://fedlex.data.admin.ch/eli/treaty/2024/0088",
            Language::De,
        )
        .await
        .unwrap();
        assert_eq!(info.title.as_deref(), Some("Convention d'application"));

        // Die Query filtert leere Literale bereits an der Quelle.
        let q = client.last_query().unwrap();
        assert!(
            q.contains(r#"STR(?title) != """#),
            "Leerstring-Filter fehlt: {q}"
        );
    }

    #[tokio::test]
    async fn unknown_process_is_not_found() {
        let client =
            MockSparqlClient::from_json(r#"{"head":{"vars":["title"]},"results":{"bindings":[]}}"#);
        let err = get_treaty_info(
            &client,
            "https://fedlex.data.admin.ch/eli/treaty/0/0",
            Language::De,
        )
        .await
        .unwrap_err();
        assert!(matches!(err, JoluxError::NotFound(_)));
    }

    #[tokio::test]
    async fn find_treaties_builds_filters_and_carries_title() {
        let client = MockSparqlClient::from_json(
            r#"{"head":{"vars":["process","title","sigDate"]},"results":{"bindings":[
              {"process":{"type":"uri","value":"https://fedlex.data.admin.ch/eli/treaty/1852/0001"},
               "title":{"type":"literal","xml:lang":"de","value":"Vertrag Y"},
               "sigDate":{"type":"literal","value":"1852-07-01"}}
            ]}}"#,
        );
        let hits = find_treaties(
            &client,
            Some("https://fedlex.data.admin.ch/vocabulary/country/136"),
            Some(true),
            5,
            Language::De,
        )
        .await
        .unwrap();
        assert_eq!(hits.len(), 1);
        // 68 §C-4: Der Treffer trägt den Titel — sonst N+1-Calls pro Hit.
        assert_eq!(hits[0].title.as_deref(), Some("Vertrag Y"));

        let q = client.last_query().unwrap();
        assert!(q.contains(
            "jolux:treatyPartyCountry <https://fedlex.data.admin.ch/vocabulary/country/136>"
        ));
        assert!(q.contains("jolux:bilateral true"));
        assert!(q.contains("LIMIT 5"));
        assert!(
            q.contains(r#"LANG(?title) = "de""#),
            "Titel muss sprach-gefiltert sein: {q}"
        );
    }

    #[test]
    fn pick_title_prefers_requested_then_official_order() {
        let titles = vec![
            ("it".to_string(), "Accordo".to_string()),
            ("fr".to_string(), "Convention".to_string()),
        ];
        assert_eq!(
            pick_title(&titles, Language::It).as_deref(),
            Some("Accordo")
        );
        // de fehlt → fr vor it (Amtssprachen-Reihenfolge).
        assert_eq!(
            pick_title(&titles, Language::De).as_deref(),
            Some("Convention")
        );
        assert_eq!(pick_title(&[], Language::De), None);
    }
}
