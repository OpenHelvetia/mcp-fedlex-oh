//! Primitive: Erlass-Suche nach Titel/Stichwort (Rulebook J3).

use crate::client::{Language, PREFIXES, SparqlClient, val};
use crate::{FEDLEX_BASE, error::JoluxError};
use fedlex_core::{ValidAsOf, swiss_today};
use serde::{Deserialize, Serialize};

/// Ein Such-Treffer: ein Erlass, der zum Suchbegriff passt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LawHit {
    /// ELI des Erlasses (relativ, `eli/cc/...`).
    pub eli: String,
    /// SR-Nummer, sofern als `historicalLegalId` am Erlass vorhanden. Neue
    /// Konsolidierungen (z. B. das nDSG, `eli/cc/2022/491`) tragen kein
    /// SR-Literal mehr — dann `None`; auflösbar über `get_law_metadata`
    /// oder rückwärts über `resolve_sr_number` (Taxonomie-Pfad). Der
    /// Taxonomie-Join wäre hier zu teuer (live +0.65 s je Suche, 68 §F-34).
    pub sr_number: Option<String>,
    /// Titel des Treffers.
    pub title: String,
    /// Amtliche Abkürzung (`jolux:titleShort`, z. B. «OR», «DSG»), sofern
    /// der Graph sie trägt (68 §F-4). Kürzel-Anfragen matchen exakt hierauf.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub abbreviation: Option<String>,
    /// Geltung **zum Stichtag `as_of`** (68 §F-3): primär aus den Datums-
    /// feldern (`entry <= as_of < min(noLonger, endApplicability)`, J3.2);
    /// fehlen sie, sagt der heutige `jolux:inForceStatus` nur für den
    /// heutigen Stichtag etwas aus — für andere Stichtage bleibt das Feld
    /// ehrlich leer (`None`) statt heutigen Status als historischen
    /// auszugeben. Dann [`check_in_force`] fragen.
    ///
    /// [`check_in_force`]: crate::temporal::check_in_force
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub in_force: Option<bool>,
    /// Verify-V2: `true`, wenn das Objekt weder Status noch irgendein
    /// Geltungs-Datum trägt — fast immer ein Publikations-Zwischenobjekt
    /// (live: `eli/cc/2020/2930_cc` neben dem kanonischen nDSG). Solche
    /// Treffer sortieren ans Gruppen-Ende; bevorzuge den kanonischen ELI.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub stub: bool,
}

// `historicalLegalId` ist OPTIONAL (68 §F-1): neue Konsolidierungen (nDSG,
// `eli/cc/2022/491`) tragen kein SR-Literal am Erlass — als Pflicht-Pattern
// schloss es genau das geltende Recht von der Titelsuche aus (der Explorer-
// Blocker: «Datenschutzgesetz» fand ausschliesslich aufgehobene Erlasse).
// `titleAlternative` matcht zusätzlich Volksnamen wie «Arbeitsgesetz», die im
// amtlichen Langtitel gar nicht vorkommen (68 §F-4); BOUND-Guard, damit die
// OPTIONAL-Variable die Fehler-Semantik des FILTER nicht kippt.
//
// Verify-L1/V1: Das LIMIT läuft über eine INNERE `SELECT ?ca … GROUP BY ?ca`-
// Subquery, damit es DISTINKTE Erlasse zählt, nicht die von den äusseren
// OPTIONALs (Status/mehrere Datumsfelder) aufgeblähten Rohzeilen. Vorher
// füllten die Duplikat-Zeilen weniger Erlasse das LIMIT-Fenster, Erlasse mit
// wörtlich passendem Titel fielen heraus UND `truncated` log (=false trotz
// weiterer Treffer). `ORDER BY DESC(COUNT(DISTINCT ?fshort)) ?ca` schiebt Erlasse MIT
// amtlicher Abkürzung (Gesetze/Verordnungen) vor titel-lose Staatsverträge —
// eine billige, ehrliche Relevanz-Ordnung; `?ca` als Tiebreak macht die
// Paginierung deterministisch.
const SEARCH_Q: &str = r#"SELECT ?ca ?sr ?title ?short ?status ?entry ?noLonger ?endApp WHERE {
  { SELECT ?ca WHERE {
      ?ca a jolux:ConsolidationAbstract ;
          jolux:isRealizedBy ?fexpr .
      ?fexpr jolux:language <__LANGURI__> ;
             jolux:title ?ftitle .
      OPTIONAL { ?fexpr jolux:titleShort ?fshort }
      OPTIONAL { ?fexpr jolux:titleAlternative ?falt }
      FILTER(
        CONTAINS(LCASE(STR(?ftitle)), LCASE("__QUERY__")) ||
        CONTAINS(LCASE(STR(?ftitle)), LCASE("__QUERY2__")) ||
        (BOUND(?falt) && (
          CONTAINS(LCASE(STR(?falt)), LCASE("__QUERY__")) ||
          CONTAINS(LCASE(STR(?falt)), LCASE("__QUERY2__"))
        ))
      )
    } GROUP BY ?ca ORDER BY DESC(COUNT(DISTINCT ?fshort)) ?ca LIMIT __LIMIT__ OFFSET __OFFSET__ }
  ?ca jolux:isRealizedBy ?expr .
  ?expr jolux:language <__LANGURI__> ;
        jolux:title ?title .
  OPTIONAL { ?expr jolux:titleShort ?short }
  OPTIONAL { ?ca jolux:historicalLegalId ?sr }
  OPTIONAL { ?ca jolux:inForceStatus ?status }
  OPTIONAL { ?ca jolux:dateEntryInForce ?entry }
  OPTIONAL { ?ca jolux:dateNoLongerInForce ?noLonger }
  OPTIONAL { ?ca jolux:dateEndApplicability ?endApp }
}"#;

// Exakte Kürzel-Auflösung über die amtliche Abkürzung (68 §F-4). Eigene,
// billige Vorabfrage (~0.4 s live) statt OR im Haupt-FILTER: dort verdrängte
// das Substring-Rauschen («OR» in «VerORdnung») den Kürzel-Treffer aus dem
// LIMIT-Fenster — live fehlte das Obligationenrecht in 20 Zeilen Rauschen.
const ABBREV_Q: &str = r#"SELECT DISTINCT ?ca ?sr ?title ?short ?status ?entry ?noLonger ?endApp WHERE {
  ?ca a jolux:ConsolidationAbstract ;
      jolux:isRealizedBy ?expr .
  ?expr jolux:language <__LANGURI__> ;
        jolux:title ?title ;
        jolux:titleShort ?short .
  OPTIONAL { ?ca jolux:historicalLegalId ?sr }
  OPTIONAL { ?ca jolux:inForceStatus ?status }
  OPTIONAL { ?ca jolux:dateEntryInForce ?entry }
  OPTIONAL { ?ca jolux:dateNoLongerInForce ?noLonger }
  OPTIONAL { ?ca jolux:dateEndApplicability ?endApp }
  FILTER(LCASE(STR(?short)) = LCASE("__QUERY__"))
} LIMIT __LIMIT__"#;

/// Sieht die Anfrage wie eine amtliche Abkürzung aus («OR», «ZGB», «ArGV 1»)?
/// Nur dann lohnt die Kürzel-Vorabfrage; lange Phrasen sind nie Kürzel.
fn looks_like_abbreviation(query: &str) -> bool {
    let t = query.trim();
    !t.is_empty() && t.chars().count() <= 12 && t.split_whitespace().count() <= 2
}

/// ASCII-Transliteration zurück zu Umlauten («ueber» → «über»), damit
/// ASCII-Anfragen Titel mit echten Umlauten treffen (68 §F-5: «ueber» fand
/// 0, «über» 1 Treffer — kommentarlos). Ersetzt ue/oe/ae nur am Wortanfang
/// oder nach Konsonant: nach Vokal ist die Folge fast immer echt (Steuer,
/// Bauer, Israel). `None`, wenn nichts zu ersetzen ist. Die Variante läuft
/// als ODER-Zweig neben der Original-Anfrage — ein Fehlgriff der Heuristik
/// kostet nur einen wirkungslosen Vergleich, nie einen Treffer.
fn umlaut_variant(query: &str) -> Option<String> {
    let chars: Vec<char> = query.chars().collect();
    let mut out = String::with_capacity(query.len());
    let mut changed = false;
    let mut i = 0;
    while i < chars.len() {
        let after_vowel = out
            .chars()
            .last()
            .is_some_and(|p| "aeiouäöüAEIOUÄÖÜ".contains(p));
        let repl = match (chars[i], chars.get(i + 1)) {
            ('u', Some('e')) if !after_vowel => Some('ü'),
            ('U', Some('e')) if !after_vowel => Some('Ü'),
            ('o', Some('e')) if !after_vowel => Some('ö'),
            ('O', Some('e')) if !after_vowel => Some('Ö'),
            ('a', Some('e')) if !after_vowel => Some('ä'),
            ('A', Some('e')) if !after_vowel => Some('Ä'),
            _ => None,
        };
        match repl {
            Some(r) => {
                out.push(r);
                changed = true;
                i += 2;
            }
            None => {
                out.push(chars[i]);
                i += 1;
            }
        }
    }
    changed.then_some(out)
}

/// Geltung eines Treffers **zum Stichtag** (68 §F-3, Doppel-Logik J3.2/J3.3).
///
/// Primär entscheiden die Datumsfelder — sie gelten für jeden Stichtag.
/// Fehlen sie, trägt der Graph nur den *heutigen* `inForceStatus`; der ist
/// ausschliesslich für den heutigen Stichtag eine Aussage. Für historische
/// Stichtage hiesse «Status heute in Kraft» eben nicht «galt damals» —
/// exakt so wählte die dokumentierte Disambiguierung im Explorer-Lauf das
/// am Stichtag noch nicht existierende DSG 2022. Dann lieber `None`.
pub(crate) fn in_force_at(
    status_uri: Option<&str>,
    entry: Option<&str>,
    no_longer: Option<&str>,
    end_app: Option<&str>,
    as_of: ValidAsOf,
) -> Option<bool> {
    if let Some(entry) = entry {
        // ISO-Datums-Strings vergleichen lexikografisch korrekt.
        let day = as_of.to_string();
        let started = entry <= day.as_str();
        let ended = [no_longer, end_app]
            .into_iter()
            .flatten()
            .any(|d| d <= day.as_str());
        Some(started && !ended)
    } else if as_of.date() == swiss_today() {
        status_uri.map(|s| s.ends_with("/0"))
    } else {
        None
    }
}

/// Sucht Erlasse, deren Titel den Suchbegriff enthält (case-insensitive).
///
/// **Live-verifiziert (2026-06-10):** Der amtliche Titel liegt auf der
/// Expression **direkt am CA** (`<CA> jolux:isRealizedBy ?expr`). Die
/// Expressions der Consolidations tragen nur technische Labels
/// (`"Consolidation: 730.0 - 2018-01-01"`) und sind für die Suche unbrauchbar.
///
/// **Discovery-Funktion ohne Provenance** — liefert Kandidaten, auf denen dann
/// provenance-tragende Primitive (`get_law_metadata`, `get_article_text`)
/// aufsetzen. Kürzel-Anfragen («OR», «ArG») lösen zuerst exakt über die
/// amtliche Abkürzung auf (68 §F-4, eigene Vorabfrage); diese Treffer stehen
/// vor den Titel-/Volksnamen-Treffern, innerhalb der Gruppen geltendes Recht
/// zuerst (68 §C-5). Der Suchbegriff wird vor der Einbettung entschärft
/// (Anführungszeichen/Backslash entfernt), damit er die Query nicht zerbricht
/// (kein SPARQL-Injection).
pub async fn search_law(
    client: &impl SparqlClient,
    query: &str,
    lang: Language,
    limit: u32,
    offset: u32,
    as_of: ValidAsOf,
) -> Result<Vec<LawHit>, JoluxError> {
    let safe = query.replace(['"', '\\'], " ");

    // Gruppe 0: exakte Kürzel-Treffer (nur wenn die Anfrage wie ein Kürzel
    // aussieht — lange Phrasen sparen sich die Vorabfrage und ihre Latenz).
    // Beim Blättern (offset > 0, 68 §F-9) entfällt sie: die Kürzel-Treffer
    // standen vollständig auf der ersten Seite.
    let mut grouped: Vec<(u8, LawHit)> = Vec::new();
    if offset == 0 && looks_like_abbreviation(&safe) {
        let sparql = format!(
            "{PREFIXES}{}",
            ABBREV_Q
                .replace("__LANGURI__", lang.vocab_uri())
                .replace("__QUERY__", safe.trim())
                .replace("__LIMIT__", &limit.to_string())
        );
        let res = client.query(&sparql).await?;
        collect_hits(&res, as_of, 0, &mut grouped);
    }

    // Gruppe 1: Substring auf Titel/Volksnamen — die Umlaut-Variante
    // («ueber» → «über») läuft als ODER-Zweig mit (68 §F-5).
    // Verify-V1: SPARQL holt MEHR als limit Zeilen, denn der Dedup (F-19)
    // schrumpft die Liste NACH dem LIMIT — vorher meldete ein volles, durch
    // Dedup geschrumpftes Fenster truncated:false, und das dahinterliegende
    // Covid-19-Gesetz war unsichtbar UND unsignalisiert. Nach dem Dedup wird
    // auf limit gekappt; ein volles Fenster ergibt wieder truncated:true.
    let overfetch = (limit.saturating_mul(2)).clamp(limit, 100);
    let variant = umlaut_variant(&safe).unwrap_or_else(|| safe.clone());
    let sparql = format!(
        "{PREFIXES}{}",
        SEARCH_Q
            .replace("__LANGURI__", lang.vocab_uri())
            .replace("__QUERY2__", &variant)
            .replace("__QUERY__", &safe)
            .replace("__LIMIT__", &overfetch.to_string())
            .replace("__OFFSET__", &offset.to_string())
    );
    let res = client.query(&sparql).await?;
    collect_hits(&res, as_of, 1, &mut grouped);

    // Kürzel-Treffer zuerst; innerhalb der Gruppen geltendes Recht zuerst
    // (68 §C-5: live stand das aufgehobene EnG 1998 VOR dem geltenden
    // EnG 2016). Stabil — die Server-Reihenfolge bleibt sonst erhalten.
    grouped.sort_by_key(|(group, h)| {
        (
            *group,
            h.stub, // Verify-V2: Zwischenobjekte hinter echte Treffer
            match h.in_force {
                Some(true) => 0u8,
                None => 1,
                Some(false) => 2,
            },
        )
    });
    let mut hits: Vec<LawHit> = grouped.into_iter().map(|(_, h)| h).collect();
    hits.truncate(limit as usize);
    Ok(hits)
}

/// Sammelt Treffer eines Resultats in `out`, dedupliziert pro ELI
/// (68 §F-19): Mehrfach-Bindings (z. B. mehrere Expressions) lieferten
/// denselben Erlass wortgleich doppelt — DISTINCT griff nicht, weil sich
/// Nebenvariablen unterscheiden. Die erste Zeile gewinnt (auch über
/// Gruppen hinweg); fehlende sr_number/abbreviation werden nachgetragen.
fn collect_hits(
    res: &crate::SparqlResults,
    as_of: ValidAsOf,
    group: u8,
    out: &mut Vec<(u8, LawHit)>,
) {
    for b in res.bindings() {
        let Some(ca) = val(b, "ca") else { continue };
        let Some(title) = val(b, "title") else {
            continue;
        };
        let stub = val(b, "status").is_none()
            && val(b, "entry").is_none()
            && val(b, "noLonger").is_none()
            && val(b, "endApp").is_none();
        let hit = LawHit {
            eli: ca.strip_prefix(FEDLEX_BASE).unwrap_or(ca).to_string(),
            sr_number: val(b, "sr").map(str::to_string),
            title: title.to_string(),
            abbreviation: val(b, "short").map(str::to_string),
            in_force: in_force_at(
                val(b, "status"),
                val(b, "entry"),
                val(b, "noLonger"),
                val(b, "endApp"),
                as_of,
            ),
            stub,
        };
        match out.iter_mut().find(|(_, h)| h.eli == hit.eli) {
            Some((_, existing)) => {
                if existing.sr_number.is_none() {
                    existing.sr_number = hit.sr_number;
                }
                if existing.abbreviation.is_none() {
                    existing.abbreviation = hit.abbreviation;
                }
            }
            None => out.push((group, hit)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::MockSparqlClient;
    use time::macros::date;

    /// Heutiger Stichtag (Schweizer Zeit) — für Tests des Status-Fallbacks.
    fn today() -> ValidAsOf {
        ValidAsOf::new(swiss_today())
    }

    const FIXTURE: &str = r#"{
      "head": {"vars": ["ca","sr","title","status","entry","noLonger","endApp"]},
      "results": {"bindings": [
        {"ca":{"type":"uri","value":"https://fedlex.data.admin.ch/eli/cc/1999/27"},
         "sr":{"type":"literal","value":"730.0"},
         "title":{"type":"literal","xml:lang":"de","value":"Energiegesetz vom 26. Juni 1998 (EnG)"},
         "status":{"type":"uri","value":"https://fedlex.data.admin.ch/vocabulary/enforcement-status/3"}},
        {"ca":{"type":"uri","value":"https://fedlex.data.admin.ch/eli/cc/2017/762"},
         "sr":{"type":"literal","value":"730.0"},
         "title":{"type":"literal","xml:lang":"de","value":"Energiegesetz (EnG)"},
         "status":{"type":"uri","value":"https://fedlex.data.admin.ch/vocabulary/enforcement-status/0"}}
      ]}
    }"#;

    /// 68 §C-5: Das ist exakt die live beobachtete Falle — das aufgehobene
    /// EnG 1998 kam VOR dem geltenden EnG 2016 (beide SR 730.0), und nichts
    /// im Treffer unterschied sie. Jetzt: in_force-Flag + geltend-zuerst.
    #[tokio::test]
    async fn hits_carry_in_force_and_current_law_sorts_first() {
        let client = MockSparqlClient::from_json(FIXTURE);
        let hits = search_law(&client, "energie", Language::De, 10, 0, today())
            .await
            .unwrap();
        assert_eq!(hits.len(), 2);
        // Das geltende Gesetz zuerst, obwohl es im Roh-Resultat an zweiter
        // Stelle stand.
        assert_eq!(hits[0].eli, "eli/cc/2017/762");
        assert_eq!(hits[0].in_force, Some(true));
        assert_eq!(hits[0].sr_number.as_deref(), Some("730.0"));
        assert_eq!(hits[1].eli, "eli/cc/1999/27");
        assert_eq!(hits[1].in_force, Some(false));

        let q = client.last_query().unwrap();
        assert!(q.contains("ConsolidationAbstract"));
        assert!(q.contains("jolux:inForceStatus"));
        assert!(q.contains("jolux:dateEntryInForce"));
        assert!(q.contains("LIMIT 20"), "Overfetch limit*2: {q}");
        assert!(q.contains(r#"LCASE("energie")"#));
    }

    /// Kein Status im Graphen (J3.3) → `None`, sortiert zwischen geltend
    /// und aufgehoben.
    #[tokio::test]
    async fn missing_status_is_none_not_guessed() {
        let client = MockSparqlClient::from_json(
            r#"{"head":{"vars":["ca","sr","title","status"]},"results":{"bindings":[
              {"ca":{"type":"uri","value":"https://fedlex.data.admin.ch/eli/cc/2000/1"},
               "sr":{"type":"literal","value":"111"},
               "title":{"type":"literal","xml:lang":"de","value":"Testgesetz"}}
            ]}}"#,
        );
        let hits = search_law(&client, "test", Language::De, 10, 0, today())
            .await
            .unwrap();
        assert_eq!(hits[0].in_force, None);
    }

    /// 68 §F-3: Die Explorer-Falle. Ein Erlass, der HEUTE gilt
    /// (Status /0, entry 2023-09-01), war am Stichtag 2020-06-01 noch nicht
    /// in Kraft — `in_force` muss den Stichtag spiegeln, nicht den heutigen
    /// Status. Der damals geltende Alt-Erlass (entry 1993, noLonger 2023)
    /// ist am Stichtag `true` und sortiert zuerst.
    #[tokio::test]
    async fn in_force_reflects_as_of_not_today() {
        let client = MockSparqlClient::from_json(
            r#"{"head":{"vars":["ca","sr","title","status","entry","noLonger","endApp"]},"results":{"bindings":[
              {"ca":{"type":"uri","value":"https://fedlex.data.admin.ch/eli/cc/2022/491"},
               "sr":{"type":"literal","value":"235.1"},
               "title":{"type":"literal","xml:lang":"de","value":"Bundesgesetz ueber den Datenschutz (DSG)"},
               "status":{"type":"uri","value":"https://fedlex.data.admin.ch/vocabulary/enforcement-status/0"},
               "entry":{"type":"literal","value":"2023-09-01"}},
              {"ca":{"type":"uri","value":"https://fedlex.data.admin.ch/eli/cc/1993/1945_1945_1945"},
               "sr":{"type":"literal","value":"235.1"},
               "title":{"type":"literal","xml:lang":"de","value":"Bundesgesetz ueber den Datenschutz (DSG)"},
               "status":{"type":"uri","value":"https://fedlex.data.admin.ch/vocabulary/enforcement-status/3"},
               "entry":{"type":"literal","value":"1993-07-01"},
               "noLonger":{"type":"literal","value":"2023-09-01"}}
            ]}}"#,
        );
        let as_of = ValidAsOf::new(date!(2020 - 06 - 01));
        let hits = search_law(&client, "Datenschutz", Language::De, 10, 0, as_of)
            .await
            .unwrap();
        // Am Stichtag gilt der Alt-Erlass — er steht zuerst.
        assert_eq!(hits[0].eli, "eli/cc/1993/1945_1945_1945");
        assert_eq!(hits[0].in_force, Some(true));
        assert_eq!(hits[1].eli, "eli/cc/2022/491");
        assert_eq!(hits[1].in_force, Some(false));
    }

    /// 68 §F-3: Ohne Datumsfelder sagt der heutige Status nichts über einen
    /// historischen Stichtag — ehrlich `None` statt heutigen Status als
    /// damalige Geltung auszugeben.
    #[tokio::test]
    async fn status_only_hit_is_unknown_for_past_as_of() {
        let client = MockSparqlClient::from_json(FIXTURE);
        let hits = search_law(
            &client,
            "energie",
            Language::De,
            10,
            0,
            ValidAsOf::new(date!(2020 - 06 - 01)),
        )
        .await
        .unwrap();
        assert!(hits.iter().all(|h| h.in_force.is_none()));
    }

    /// 68 §F-4 (Explorer-Falle): «OR» fand Rheinschiffe-Verordnungen
    /// («VerORdnung» als Substring), das Obligationenrecht fehlte. Jetzt
    /// löst die Kürzel-Vorabfrage exakt über titleShort auf und der Treffer
    /// steht VOR dem Substring-Rauschen.
    #[tokio::test]
    async fn abbreviation_resolves_via_title_short_and_ranks_first() {
        let abbrev = r#"{"head":{"vars":["ca","sr","title","short","status","entry"]},"results":{"bindings":[
          {"ca":{"type":"uri","value":"https://fedlex.data.admin.ch/eli/cc/27/317_321_377"},
           "sr":{"type":"literal","value":"220"},
           "title":{"type":"literal","xml:lang":"de","value":"Bundesgesetz betreffend die Ergaenzung des Schweizerischen Zivilgesetzbuches (Obligationenrecht)"},
           "short":{"type":"literal","value":"OR"},
           "entry":{"type":"literal","value":"1912-01-01"}}
        ]}}"#;
        let noise = r#"{"head":{"vars":["ca","sr","title","status","entry"]},"results":{"bindings":[
          {"ca":{"type":"uri","value":"https://fedlex.data.admin.ch/eli/cc/1960/433_465_451"},
           "sr":{"type":"literal","value":"747.224.231"},
           "title":{"type":"literal","xml:lang":"de","value":"Verordnung ueber die Untersuchung der Rheinschiffe"},
           "entry":{"type":"literal","value":"1960-05-15"}}
        ]}}"#;
        let client = MockSparqlClient::from_json_sequence(&[abbrev, noise]);
        let hits = search_law(&client, "OR", Language::De, 10, 0, today())
            .await
            .unwrap();
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].eli, "eli/cc/27/317_321_377");
        assert_eq!(hits[0].abbreviation.as_deref(), Some("OR"));
        assert_eq!(hits[1].eli, "eli/cc/1960/433_465_451");
        // Zwei Queries: zuerst die exakte Kürzel-Auflösung, dann Substring.
        let qs = client.queries();
        assert_eq!(qs.len(), 2);
        assert!(qs[0].contains("jolux:titleShort ?short ."));
        assert!(qs[0].contains(r#"LCASE(STR(?short)) = LCASE("OR")"#));
        assert!(qs[1].contains("CONTAINS(LCASE(STR(?ftitle))"));
    }

    /// 68 §F-9: offset landet in der Substring-Query; die Kürzel-Vorabfrage
    /// entfällt beim Blättern (ihre Treffer standen komplett auf Seite 1).
    #[tokio::test]
    async fn offset_pages_substring_query_and_skips_abbrev() {
        let client = MockSparqlClient::from_json(FIXTURE);
        let _ = search_law(&client, "OR", Language::De, 10, 20, today())
            .await
            .unwrap();
        let qs = client.queries();
        assert_eq!(qs.len(), 1, "keine Kürzel-Vorabfrage beim Blättern");
        assert!(qs[0].contains("LIMIT 20 OFFSET 20"), "{}", qs[0]);
    }

    /// Lange Phrasen sind nie Kürzel — keine Vorabfrage, keine Extra-Latenz.
    #[tokio::test]
    async fn long_phrase_skips_abbreviation_query() {
        let client = MockSparqlClient::from_json(FIXTURE);
        let _ = search_law(
            &client,
            "Bundesgesetz über den Datenschutz",
            Language::De,
            10,
            0,
            today(),
        )
        .await
        .unwrap();
        assert_eq!(client.queries().len(), 1);
        // Volksnamen matchen zusätzlich über titleAlternative.
        assert!(client.queries()[0].contains("jolux:titleAlternative ?falt"));
    }

    /// 68 §F-1 (Explorer-Blocker): Neue Konsolidierungen ohne SR-Literal
    /// (nDSG) müssen per Titel auffindbar sein — `historicalLegalId` ist
    /// OPTIONAL, `sr_number` bleibt dann leer statt den Treffer zu schlucken.
    #[tokio::test]
    async fn finds_law_without_sr_literal() {
        let client = MockSparqlClient::from_json(
            r#"{"head":{"vars":["ca","sr","title","status","entry"]},"results":{"bindings":[
              {"ca":{"type":"uri","value":"https://fedlex.data.admin.ch/eli/cc/2022/491"},
               "title":{"type":"literal","xml:lang":"de","value":"Bundesgesetz ueber den Datenschutz (Datenschutzgesetz, DSG)"},
               "status":{"type":"uri","value":"https://fedlex.data.admin.ch/vocabulary/enforcement-status/0"},
               "entry":{"type":"literal","value":"2023-09-01"}}
            ]}}"#,
        );
        let hits = search_law(&client, "Datenschutzgesetz", Language::De, 10, 0, today())
            .await
            .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].eli, "eli/cc/2022/491");
        assert_eq!(hits[0].sr_number, None);
        assert_eq!(hits[0].in_force, Some(true));
        // Das Pflicht-Pattern ist wirklich weg.
        let q = client.last_query().unwrap();
        assert!(q.contains("OPTIONAL { ?ca jolux:historicalLegalId ?sr }"));
    }

    /// 68 §F-19: Mehrfach-Bindings desselben Erlasses (z. B. über mehrere
    /// Expressions) kollabieren zu EINEM Treffer; sr_number wird gemerged.
    #[tokio::test]
    async fn duplicate_rows_collapse_to_one_hit() {
        let client = MockSparqlClient::from_json(
            r#"{"head":{"vars":["ca","sr","title","status","entry"]},"results":{"bindings":[
              {"ca":{"type":"uri","value":"https://fedlex.data.admin.ch/eli/cc/2019/112"},
               "title":{"type":"literal","xml:lang":"de","value":"Schengen-Datenschutzgesetz"},
               "entry":{"type":"literal","value":"2019-03-01"}},
              {"ca":{"type":"uri","value":"https://fedlex.data.admin.ch/eli/cc/2019/112"},
               "sr":{"type":"literal","value":"235.3"},
               "title":{"type":"literal","xml:lang":"de","value":"Schengen-Datenschutzgesetz"},
               "entry":{"type":"literal","value":"2019-03-01"}}
            ]}}"#,
        );
        let hits = search_law(&client, "Datenschutz", Language::De, 10, 0, today())
            .await
            .unwrap();
        assert_eq!(hits.len(), 1, "Duplikat nicht kollabiert: {hits:?}");
        // Die sr_number aus der zweiten Zeile ist nachgetragen.
        assert_eq!(hits[0].sr_number.as_deref(), Some("235.3"));
    }

    /// 68 §F-5: «ueber» fand 0, «über» 1 Treffer. Die ASCII-Anfrage nimmt
    /// jetzt die Umlaut-Variante als ODER-Zweig mit; echte u+e-Folgen nach
    /// Vokal (Steuer, Bauer) bleiben unangetastet.
    #[test]
    fn umlaut_variant_transliterate_rules() {
        assert_eq!(
            umlaut_variant("Bundesgesetz ueber den Datenschutz").as_deref(),
            Some("Bundesgesetz über den Datenschutz")
        );
        assert_eq!(umlaut_variant("Ueber").as_deref(), Some("Über"));
        assert_eq!(umlaut_variant("Waelder").as_deref(), Some("Wälder"));
        assert_eq!(umlaut_variant("Gehoer").as_deref(), Some("Gehör"));
        // Nach Vokal ist ue/ae echt — keine Ersetzung, also keine Variante.
        assert_eq!(umlaut_variant("Steuer"), None);
        assert_eq!(umlaut_variant("Bauer"), None);
        assert_eq!(umlaut_variant("Datenschutz"), None);
    }

    #[tokio::test]
    async fn ascii_query_carries_umlaut_variant_in_filter() {
        let client = MockSparqlClient::from_json(FIXTURE);
        let _ = search_law(
            &client,
            "Bundesgesetz ueber den Datenschutz",
            Language::De,
            10,
            0,
            today(),
        )
        .await
        .unwrap();
        let q = client.last_query().unwrap();
        assert!(q.contains("Bundesgesetz ueber den Datenschutz"));
        assert!(q.contains("Bundesgesetz über den Datenschutz"));
    }

    /// Verify-V1: Der F-19-Dedup schrumpfte die Liste NACH dem SPARQL-LIMIT —
    /// ein volles Fenster meldete truncated:false und der Rest (Covid-19-
    /// Gesetz) war unsichtbar UND unsignalisiert. Jetzt: Overfetch, Kappung
    /// auf limit erst nach dem Dedup.
    #[tokio::test]
    async fn overfetch_keeps_truncated_honest_after_dedup() {
        // 3 Rohzeilen, davon 2 Duplikate derselben ELI -> 2 unique Hits.
        // limit=2: SPARQL muss MEHR als 2 anfordern (LIMIT 4) und die
        // Kappung passiert nach dem Dedup.
        let rows = r#"{"head":{"vars":["ca","sr","title","status","entry"]},"results":{"bindings":[
          {"ca":{"type":"uri","value":"https://fedlex.data.admin.ch/eli/cc/2020/195"},
           "title":{"type":"literal","xml:lang":"de","value":"COVID-19-Verordnung Miete"},
           "entry":{"type":"literal","value":"2020-03-27"}},
          {"ca":{"type":"uri","value":"https://fedlex.data.admin.ch/eli/cc/2020/195"},
           "title":{"type":"literal","xml:lang":"de","value":"COVID-19-Verordnung Miete"},
           "entry":{"type":"literal","value":"2020-03-27"}},
          {"ca":{"type":"uri","value":"https://fedlex.data.admin.ch/eli/cc/2020/711"},
           "title":{"type":"literal","xml:lang":"de","value":"Covid-19-Gesetz"},
           "entry":{"type":"literal","value":"2020-09-26"}}
        ]}}"#;
        let client = MockSparqlClient::from_json(rows);
        let hits = search_law(
            &client,
            "Covid-19 Epidemie Massnahmen",
            Language::De,
            2,
            0,
            today(),
        )
        .await
        .unwrap();
        assert_eq!(hits.len(), 2, "{hits:?}");
        let q = client.last_query().unwrap();
        assert!(q.contains("LIMIT 4"), "Overfetch fehlt: {q}");
    }

    /// Verify-V2: Publikations-Zwischenobjekte (weder Status noch Datum)
    /// werden als stub markiert und sortieren hinter echte Treffer.
    #[tokio::test]
    async fn stub_objects_are_flagged_and_sort_last() {
        let rows = r#"{"head":{"vars":["ca","sr","title","status","entry"]},"results":{"bindings":[
          {"ca":{"type":"uri","value":"https://fedlex.data.admin.ch/eli/cc/2020/2930_cc"},
           "title":{"type":"literal","xml:lang":"de","value":"Bundesgesetz ueber den Datenschutz (DSG)"}},
          {"ca":{"type":"uri","value":"https://fedlex.data.admin.ch/eli/cc/2022/491"},
           "title":{"type":"literal","xml:lang":"de","value":"Bundesgesetz ueber den Datenschutz (DSG)"},
           "status":{"type":"uri","value":"https://fedlex.data.admin.ch/vocabulary/enforcement-status/0"},
           "entry":{"type":"literal","value":"2023-09-01"}}
        ]}}"#;
        let client = MockSparqlClient::from_json(rows);
        let hits = search_law(
            &client,
            "Datenschutz und mehr Woerter",
            Language::De,
            10,
            0,
            today(),
        )
        .await
        .unwrap();
        assert_eq!(hits[0].eli, "eli/cc/2022/491");
        assert!(!hits[0].stub);
        assert_eq!(hits[1].eli, "eli/cc/2020/2930_cc");
        assert!(hits[1].stub, "{hits:?}");
    }

    #[tokio::test]
    async fn neutralizes_injection_in_query() {
        let client = MockSparqlClient::from_json(FIXTURE);
        let _ = search_law(&client, r#"a") } INJECT {"#, Language::De, 5, 0, today())
            .await
            .unwrap();
        let q = client.last_query().unwrap();
        // Die Breakout-Sequenz (Quote+schliessende Klammer) darf nicht roh vorkommen ...
        assert!(!q.contains("\") }"), "Breakout-Sequenz nicht neutralisiert");
        // ... der Text bleibt aber als harmloses Literal im CONTAINS erhalten.
        assert!(q.contains("INJECT"));
    }
}
