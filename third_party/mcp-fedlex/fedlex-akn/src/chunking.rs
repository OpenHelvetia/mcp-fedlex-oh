//! Primitive: Hollowing & RAG-Chunking (Lexikon AKN-CHK-01/02, Rulebook X14/X20).
//!
//! **Zwei Textsichten, strikt getrennt (SOTA-T13, Strategie V3):** Die
//! TXT-Primitive (`text.rs`, `dom::text_of`) liefern den authentischen,
//! zitierfähigen Normtext — der Chunk-Text hier ist die **angereicherte
//! Retrieval-Sicht** (Markdown-Tabellen, `[Historie: …]`, ref-Links,
//! `[Formel]`/`[Grafik]`-Platzhalter). Der private Renderer
//! ([`chunk_text_of`]) lebt deshalb bewusst in diesem Modul und fasst die
//! `text.rs`-Primitive nicht an (Wächter:
//! `chunk_enrichment_never_leaks_into_reader_text`).

use crate::components::{ComponentInfo, get_component_document, list_components};
use crate::doc::{DocPattern, classify_pattern, get_frbr_metadata};
use crate::dom::{AknDocument, Content, NodeId};
use crate::error::AknError;
use crate::special::{ForeignKind, classify_foreign};
use crate::structure::section_path_of;
use crate::text::has_eid_descendant;
use serde::{Deserialize, Serialize};

/// Ein Element der ausgehöhlten Dokument-Sicht.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HollowedElement {
    /// eId des Elements.
    pub eid: String,
    /// Element-Typ.
    pub kind: String,
    /// `true` = eId-Blatt (kein eId-Nachfahre) — trägt den echten Text.
    pub is_leaf: bool,
    /// Blatt: Normtext. Eltern-Container: Platzhalter mit den direkten
    /// eId-Kindern.
    pub text: String,
}

/// AKN-CHK-01: Höhlt das Dokument aus — nur eId-Blätter behalten ihren Text,
/// Eltern-Container werden zu Platzhaltern.
///
/// Eltern-Texte sind die Konkatenation ihrer Kinder (87.1 % Redundanz, X20.2).
/// Wer naiv alle eId-Elemente als Chunks indexiert, hat jeden Satz 3-4× im
/// Index. Beim Energiegesetz: 117'647 → ~15'156 Zeichen (X20.1).
pub fn hollow_document(doc: &AknDocument) -> Vec<HollowedElement> {
    let mut entries: Vec<(NodeId, &str)> = doc
        .all_eids()
        .flat_map(|(eid, nodes)| nodes.iter().map(move |&n| (n, eid)))
        .collect();
    entries.sort_unstable_by_key(|&(n, _)| n);
    entries
        .into_iter()
        .map(|(node, eid)| {
            let is_leaf = !has_eid_descendant(doc, node);
            let text = if is_leaf {
                doc.text_of(node)
            } else {
                let children = direct_eid_children(doc, node).join(", ");
                format!("[Siehe Unterelemente: {children}]")
            };
            HollowedElement {
                eid: eid.to_string(),
                kind: doc.tag(node).to_string(),
                is_leaf,
                text,
            }
        })
        .collect()
}

/// Nächste eId-tragende Nachfahren (BFS, stoppt an jedem eId-Knoten).
fn direct_eid_children(doc: &AknDocument, node: NodeId) -> Vec<String> {
    let mut out = Vec::new();
    let mut queue: Vec<NodeId> = doc.children(node).collect();
    let mut i = 0;
    while i < queue.len() {
        let c = queue[i];
        i += 1;
        match doc.eid(c) {
            Some(eid) => out.push(eid.to_string()),
            None => queue.extend(doc.children(c)),
        }
    }
    out
}

/// Die 8 Pflicht-Metadaten eines RAG-Chunks (X14.3) plus die
/// Überschriften-Texte des Gliederungspfads (Strategie V3/V4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkMetadata {
    /// SR-Nummer ohne Präfix (`730.0`).
    pub sr: Option<String>,
    /// Erlass-Titel.
    pub title: Option<String>,
    /// Work-URI (`/eli/cc/2017/762`).
    pub eli: Option<String>,
    /// Sprache der Expression.
    pub language: Option<String>,
    /// `jolux:dateDocument` aus dem FRBR-Block.
    pub date: Option<String>,
    /// Gliederungs-Pfad als eId-Liste.
    pub section_path: Vec<String>,
    /// Überschriften-Texte zum Gliederungspfad (`num` + `heading` je Stufe,
    /// z.B. `"1. Kapitel: Allgemeine Bestimmungen"`, `"Art. 1 Zweck"`) —
    /// Baumaterial für den S3-Embedding-Input der semantic-Seite
    /// (`title + section_headings + text`, Strategie V4). Stufen ohne
    /// `num`/`heading` entfallen; die Liste ist deshalb NICHT zwingend
    /// deckungsgleich mit `section_path`. `serde(default)` hält alte
    /// Payloads deserialisierbar.
    #[serde(default)]
    pub section_headings: Vec<String>,
    /// eId der Chunk-Quelle.
    pub eid: Option<String>,
    /// Sammlung aus dem ELI-Pfad (`cc`, `oc`, `fga`).
    pub collection: Option<String>,
}

/// Ein RAG-Chunk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Chunk {
    /// Stabile ID: `{eli}#{eid}` bzw. `{eli}#idx{n}`.
    pub chunk_id: String,
    /// Chunk-Text — die angereicherte **Retrieval-Sicht** (Markdown-Tabellen,
    /// `[Historie: …]`, ref-Links, Strategie V3), NICHT der zitierfähige
    /// Normtext. Zitate liefern die TXT-Primitive.
    pub text: String,
    /// Die Pflicht-Metadaten.
    pub metadata: ChunkMetadata,
}

/// Schwelle für Artikel-Splitting (Artikel-Median liegt bei ~550 Zeichen,
/// X14.2 — was darüber hinausschiesst, wird pro Absatz gesplittet).
const SPLIT_THRESHOLD: usize = 2000;

/// AKN-CHK-02: Zerlegt das Dokument musterabhängig in RAG-Chunks (X14.1).
///
/// STRUCTURED/FLAT_ARTICLES → pro Artikel (Übergrosse pro Absatz),
/// LEVEL_BASED → pro Level-Blatt, AMENDMENT → pro `<mod>`,
/// NO_BODY → pro Nicht-Stub-Component, OTHER → `<p>`-Gruppen.
pub fn chunk_document(doc: &AknDocument) -> Result<Vec<Chunk>, AknError> {
    let meta = get_frbr_metadata(doc)?;
    let info = classify_pattern(doc);
    let base = BaseMeta::from_frbr(&meta);
    let mut chunks = Vec::new();
    // Scope auf den Body — Components haben eigene FRBR-Werke (X19.8) und
    // werden im NO_BODY-Zweig bzw. via CMP-02 separat gechunkt.
    let body = doc
        .find_child(doc.root(), "body")
        .or_else(|| doc.find_child(doc.root(), "mainBody"))
        .unwrap_or_else(|| doc.root());

    match info.pattern {
        DocPattern::Structured | DocPattern::FlatArticles => {
            for art in doc.find_all(body, "article") {
                let text = chunk_text_of(doc, art);
                let paras: Vec<NodeId> = doc
                    .children(art)
                    .filter(|&c| doc.tag(c) == "paragraph")
                    .collect();
                // Split nur, wenn es auch Absätze gibt — ein übergrosser
                // Artikel ohne direkte paragraph-Kinder bleibt sonst EIN
                // Chunk statt stillschweigend zu verschwinden.
                if text.chars().count() > SPLIT_THRESHOLD && !paras.is_empty() {
                    for para in paras {
                        push_chunks_table_aware(&mut chunks, doc, &base, para);
                    }
                } else {
                    push_chunks_table_aware(&mut chunks, doc, &base, art);
                }
            }
        }
        DocPattern::LevelBased => {
            for lvl in doc.find_all(body, "level") {
                // Nur Level-Blätter (kein Kind-Level) — Eltern sind redundant (X20.2).
                if doc.find_all(lvl, "level").len() == 1 {
                    push_chunks_table_aware(&mut chunks, doc, &base, lvl);
                }
            }
        }
        DocPattern::Amendment => {
            for m in doc.find_all(body, "mod") {
                push_chunks_table_aware(&mut chunks, doc, &base, m);
            }
        }
        DocPattern::NoBody => {
            for comp in list_components(doc) {
                if comp.is_empty_stub {
                    continue;
                }
                let inner = get_component_document(doc, comp.index)?;
                // Component rekursiv chunken — eigenes FRBR-Werk (X19.8).
                // Der Kontext-Kopf stellt Anhang-Titel + Bezugserlass wieder
                // her (Strategie V3): das Anhang-Werk selbst kennt seinen
                // Träger-Erlass nicht, ohne Kopf wäre der Chunk kontextlos.
                let header = component_context_header(&comp, &base);
                for mut c in chunk_document(&inner)? {
                    c.text = format!("{header}\n{}", c.text);
                    chunks.push(c);
                }
            }
        }
        DocPattern::Other => {
            let mut group = String::new();
            for p in doc.find_all(body, "p") {
                let t = chunk_text_of(doc, p);
                if !group.is_empty() && group.chars().count() + t.chars().count() > SPLIT_THRESHOLD
                {
                    push_chunk(&mut chunks, doc, &base, None, std::mem::take(&mut group));
                }
                if !t.is_empty() {
                    if !group.is_empty() {
                        group.push('\n');
                    }
                    group.push_str(&t);
                }
            }
            if !group.is_empty() {
                push_chunk(&mut chunks, doc, &base, None, group);
            }
        }
    }
    Ok(chunks)
}

/// Kontext-Kopf für Anhang-Chunks (Strategie V3): Anhänge sind eigene
/// FRBR-Werke (X19.8) — Titel des Bezugserlasses und SR-Nummer machen den
/// Chunk im Retrieval wieder zuordenbar.
fn component_context_header(comp: &ComponentInfo, base: &BaseMeta) -> String {
    let annex = comp
        .title
        .as_deref()
        .or(comp.doc_name.as_deref())
        .unwrap_or("Anhang");
    let parent = match (&base.title, &base.eli) {
        (Some(t), _) => Some(t.clone()),
        (None, Some(e)) => Some(e.clone()),
        (None, None) => None,
    };
    match (parent, &base.sr) {
        (Some(p), Some(sr)) => format!("[{annex} — Anhang zu: {p} (SR {sr})]"),
        (Some(p), None) => format!("[{annex} — Anhang zu: {p}]"),
        (None, _) => format!("[{annex}]"),
    }
}

struct BaseMeta {
    sr: Option<String>,
    title: Option<String>,
    eli: Option<String>,
    language: Option<String>,
    date: Option<String>,
    collection: Option<String>,
}

impl BaseMeta {
    fn from_frbr(m: &crate::doc::FrbrMetadata) -> Self {
        // Datierte Konsolidierungs-URI auf Work-Ebene reduzieren — Chunk-IDs
        // müssen über Fassungen stabil und mit JOLux joinbar sein.
        let work = crate::doc::work_eli_path(&m.eli_work).map(str::to_string);
        let collection = work
            .as_deref()
            .and_then(|p| p.strip_prefix("eli/"))
            .and_then(|rest| rest.split('/').next())
            .map(str::to_string);
        let date = m
            .dates
            .iter()
            .find(|(n, _)| n == "jolux:dateDocument")
            .or_else(|| m.dates.first())
            .map(|(_, d)| d.clone());
        BaseMeta {
            sr: m
                .sr_number
                .as_deref()
                .map(|s| s.trim_start_matches("SR ").to_string()),
            title: m.title.clone(),
            eli: work.or_else(|| Some(m.eli_work.clone())),
            language: m.language.clone(),
            date,
            collection,
        }
    }
}

fn push_chunk(
    chunks: &mut Vec<Chunk>,
    doc: &AknDocument,
    base: &BaseMeta,
    node: Option<NodeId>,
    text: String,
) {
    push_chunk_suffixed(chunks, doc, base, node, None, text);
}

/// Chunkt einen Knoten tabellen-bewusst (X13.3).
///
/// Sprengt der Knotentext die Split-Schwelle und enthält er Tabellen, wird
/// der Fliesstext ohne Tabellen EIN Chunk und jede Tabelle ein eigener.
/// Übergrosse Tabellen werden zeilengruppenweise gesplittet, der
/// Markdown-Kopf (Kopfzeile + Separator) wird jeder Gruppe vorangestellt,
/// damit die Spalten lesbar bleiben. Tabellen sind semantische Einheiten
/// und werden nie mitten in der Zeile getrennt (X13.2).
fn push_chunks_table_aware(
    chunks: &mut Vec<Chunk>,
    doc: &AknDocument,
    base: &BaseMeta,
    node: NodeId,
) {
    let text = chunk_text_of(doc, node);
    // Nur Top-Level-Tabellen separat chunken — eine verschachtelte Tabelle
    // ist bereits (geflattet) im Zellentext ihrer Elterntabelle enthalten.
    let tables: Vec<NodeId> = doc
        .find_all(node, "table")
        .into_iter()
        .filter(|&t| !has_table_ancestor_below(doc, t, node))
        .collect();
    if text.chars().count() <= SPLIT_THRESHOLD || tables.is_empty() {
        push_chunk(chunks, doc, base, Some(node), text);
        return;
    }

    // Fliesstext ohne Tabellen behält die Stamm-Chunk-ID des Knotens.
    push_chunk(
        chunks,
        doc,
        base,
        Some(node),
        chunk_text_without_tables(doc, node),
    );

    for (t_idx, &table) in tables.iter().enumerate() {
        let suffix = format!("tbl{}", t_idx + 1);
        let table_text = table_markdown(doc, table);
        if table_text.chars().count() <= SPLIT_THRESHOLD {
            push_chunk_suffixed(chunks, doc, base, Some(node), Some(&suffix), table_text);
            continue;
        }

        // Übergross. Zeilengruppen bilden, Markdown-Kopf wiederholen.
        let trs = table_rows(doc, table);
        let header = trs
            .first()
            .filter(|&&tr| doc.children(tr).any(|c| doc.tag(c) == "th"))
            .and_then(|&tr| {
                let row = markdown_row(doc, tr)?;
                Some(format!(
                    "{row}\n{}",
                    markdown_separator(cell_count(doc, tr))
                ))
            });
        let data_rows: Vec<String> = trs
            .iter()
            .skip(usize::from(header.is_some()))
            .filter_map(|&tr| markdown_row(doc, tr))
            .collect();

        let header_len = header.as_ref().map_or(0, |h| h.chars().count() + 1);
        let mut group = String::new();
        let mut part = 1usize;
        for row in data_rows {
            let would_be = header_len + group.chars().count() + row.chars().count() + 1;
            if !group.is_empty() && would_be > SPLIT_THRESHOLD {
                flush_table_group(
                    chunks,
                    doc,
                    base,
                    node,
                    &suffix,
                    &mut part,
                    header.as_deref(),
                    std::mem::take(&mut group),
                );
            }
            if !group.is_empty() {
                group.push('\n');
            }
            group.push_str(&row);
        }
        if !group.is_empty() {
            flush_table_group(
                chunks,
                doc,
                base,
                node,
                &suffix,
                &mut part,
                header.as_deref(),
                group,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn flush_table_group(
    chunks: &mut Vec<Chunk>,
    doc: &AknDocument,
    base: &BaseMeta,
    node: NodeId,
    suffix: &str,
    part: &mut usize,
    header: Option<&str>,
    group: String,
) {
    let text = match header {
        Some(h) => format!("{h}\n{group}"),
        None => group,
    };
    let part_suffix = format!("{suffix}/part{part}");
    push_chunk_suffixed(chunks, doc, base, Some(node), Some(&part_suffix), text);
    *part += 1;
}

fn push_chunk_suffixed(
    chunks: &mut Vec<Chunk>,
    doc: &AknDocument,
    base: &BaseMeta,
    node: Option<NodeId>,
    suffix: Option<&str>,
    text: String,
) {
    if text.is_empty() {
        return;
    }
    let eid = node.and_then(|n| doc.eid(n)).map(str::to_string);
    let steps = node.map(|n| section_path_of(doc, n)).unwrap_or_default();
    let section_path: Vec<String> = steps.iter().filter_map(|s| s.eid.clone()).collect();
    // Überschriften-Texte zum Pfad (Strategie V3/V4): `num` + `heading` je
    // Stufe; Stufen ohne beides (z.B. nackte paragraphs) entfallen.
    // Nur ECHTE Eltern-Stufen (Thesis-Semantik `parent_heading`): die eigene
    // Nummer/Überschrift trägt der Chunk-Text bereits — als Kontext dupliziert
    // würde sie im Embedding-Input die Unterscheidungs-Token verwässern
    // (am Hash-Embedder gemessen: Kollisions-Score schlug den echten Match).
    let section_headings: Vec<String> = steps
        .iter()
        .filter(|s| s.eid.as_deref() != eid.as_deref())
        .filter_map(|s| {
            let label = [s.num.as_deref(), s.heading.as_deref()]
                .into_iter()
                .flatten()
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .collect::<Vec<_>>()
                .join(" ");
            (!label.is_empty()).then_some(label)
        })
        .collect();
    let eli = base.eli.clone().unwrap_or_default();
    let stem = match &eid {
        Some(e) => format!("{eli}#{e}"),
        None => format!("{eli}#idx{}", chunks.len()),
    };
    let chunk_id = match suffix {
        Some(s) => format!("{stem}/{s}"),
        None => stem,
    };
    chunks.push(Chunk {
        chunk_id,
        text,
        metadata: ChunkMetadata {
            sr: base.sr.clone(),
            title: base.title.clone(),
            eli: base.eli.clone(),
            language: base.language.clone(),
            date: base.date.clone(),
            section_path,
            section_headings,
            eid,
            collection: base.collection.clone(),
        },
    });
}

// ───────────────── Chunk-Text-Renderer (nur AKN-CHK-02) ─────────────────
//
// Angereicherte Retrieval-Sicht nach Thesis-STRATEGY §5 (Strategie V3):
// Markdown-Tabellen, `[Historie: …]` statt Fussnoten-Löschung, ELI-refs als
// Markdown-Links, `[Formel]`/`[Grafik]` statt `<foreign>`-Rauschen. Bewusst
// getrennt von `dom::text_of`/`text.rs` — die liefern den zitierfähigen
// Normtext und bleiben unangetastet (Zitierfähigkeits-Leitplanke).

/// Rendert den Chunk-Text eines Teilbaums (Blockstruktur wie `text_of`,
/// plus die V3-Anreicherungen).
fn chunk_text_of(doc: &AknDocument, node: NodeId) -> String {
    let mut out = String::new();
    let mut notes = Vec::new();
    write_chunk_text(doc, node, &mut out, &mut notes, false);
    end_block(&mut out, &mut notes);
    tidy_chunk(&out)
}

/// Wie [`chunk_text_of`], aber ohne `<table>`-Teilbäume — für den
/// Prosa-Chunk beim Tabellen-Splitting (X13.3).
fn chunk_text_without_tables(doc: &AknDocument, node: NodeId) -> String {
    let mut out = String::new();
    let mut notes = Vec::new();
    write_chunk_text(doc, node, &mut out, &mut notes, true);
    end_block(&mut out, &mut notes);
    tidy_chunk(&out)
}

/// Rekursiver Kern des Renderers. `notes` sammelt `[Historie: …]`-Marker
/// der laufenden Zeile: Fussnoten stehen im Corpus mitten im Wort (X6.4) —
/// inline an Ort und Stelle würden sie Normwörter zerreissen
/// («Energie[Historie: …]verbrauch»), deshalb wandern sie ans Blockende.
fn write_chunk_text(
    doc: &AknDocument,
    id: NodeId,
    out: &mut String,
    notes: &mut Vec<String>,
    skip_tables: bool,
) {
    for c in doc.content(id) {
        match c {
            Content::Text(t) => push_text_normalized(out, t),
            Content::Element(e) => {
                let e = *e;
                match doc.tag(e) {
                    "authorialNote" => {
                        if let Some(h) = note_history(doc, e) {
                            notes.push(h);
                        }
                    }
                    "foreign" => {
                        out.push(' ');
                        out.push_str(&foreign_placeholder(doc, e));
                        out.push(' ');
                    }
                    "ref" => out.push_str(&ref_markdown(doc, e)),
                    "table" => {
                        if !skip_tables {
                            end_block(out, notes);
                            append_table_markdown(doc, e, out);
                        }
                    }
                    tag => {
                        write_chunk_text(doc, e, out, notes, skip_tables);
                        match tag {
                            "num" | "td" | "th" => out.push(' '),
                            "p" | "paragraph" | "heading" | "item" | "listIntroduction"
                            | "intro" | "tr" | "block" | "content" => end_block(out, notes),
                            _ => {}
                        }
                    }
                }
            }
        }
    }
}

/// Blockgrenze: aufgelaufene `[Historie: …]`-Marker anhängen, dann Umbruch.
fn end_block(out: &mut String, notes: &mut Vec<String>) {
    for n in notes.drain(..) {
        out.push(' ');
        out.push_str(&n);
    }
    out.push('\n');
}

/// Fussnote als kompakter Historie-Marker (Änderungshistorie ist Suchsignal
/// für temporale Fragen, Strategie V3). Leere Notes entfallen.
fn note_history(doc: &AknDocument, note: NodeId) -> Option<String> {
    let text = inline_markdown(doc, note);
    (!text.is_empty()).then(|| format!("[Historie: {text}]"))
}

/// `<ref>` als Markdown-Link, sofern der href eine ELI trägt — ELI-URIs im
/// Chunk machen Cross-Law-Kandidaten sichtbar (Strategie V3). Andere hrefs
/// (und die 15 % href-losen refs, X11.2) bleiben nackter Text.
fn ref_markdown(doc: &AknDocument, r: NodeId) -> String {
    let label = inline_markdown(doc, r);
    match doc.attr(r, "href") {
        Some(href) if href.contains("/eli/") && !label.is_empty() => {
            format!("[{label}]({href})")
        }
        _ => label,
    }
}

/// `<foreign>`-Insel: Alt-Text falls vorhanden, sonst Platzhalter — kein
/// Binär-/Markup-Rauschen im Embedding (Strategie V3, X18.4).
/// MathML/OOXML sind im Corpus Formelträger → `[Formel]`; SVG und der Rest
/// → `[Grafik]`.
fn foreign_placeholder(doc: &AknDocument, f: NodeId) -> String {
    if let Some(alt) = foreign_alt_text(doc, f) {
        return alt;
    }
    match classify_foreign(doc, f).0 {
        ForeignKind::MathMl | ForeignKind::Ooxml => "[Formel]".to_string(),
        _ => "[Grafik]".to_string(),
    }
}

/// Alt-Text einer `<foreign>`-Insel: `@alttext` (MathML) bzw.
/// `<title>`/`<desc>` (SVG) — innerhalb von `<foreign>` kollidieren die
/// Namen nicht mit den AKN-Elementen gleichen Namens.
fn foreign_alt_text(doc: &AknDocument, f: NodeId) -> Option<String> {
    for n in doc.descendants(f) {
        if let Some(alt) = doc.attr(n, "alttext") {
            let alt = collapse_inline(alt);
            if !alt.is_empty() {
                return Some(alt);
            }
        }
    }
    for tag in ["title", "desc"] {
        for n in doc.find_all(f, tag) {
            let t = collapse_inline(&doc.text_of(n));
            if !t.is_empty() {
                return Some(t);
            }
        }
    }
    None
}

/// Rendert einen Teilbaum als EINE Zeile (Zelltexte, Fussnoten, ref-Labels).
/// Gleiche Anreicherungen wie der Block-Renderer, Blockgrenzen werden zu
/// Leerzeichen; Fussnoten wandern ans Ende (Wort-Integrität, X6.4).
fn inline_markdown(doc: &AknDocument, id: NodeId) -> String {
    let mut out = String::new();
    let mut notes = Vec::new();
    write_inline(doc, id, &mut out, &mut notes);
    for n in notes.drain(..) {
        out.push(' ');
        out.push_str(&n);
    }
    collapse_inline(&out)
}

fn write_inline(doc: &AknDocument, id: NodeId, out: &mut String, notes: &mut Vec<String>) {
    for c in doc.content(id) {
        match c {
            Content::Text(t) => push_text_normalized(out, t),
            Content::Element(e) => {
                let e = *e;
                match doc.tag(e) {
                    "authorialNote" => {
                        if let Some(h) = note_history(doc, e) {
                            notes.push(h);
                        }
                    }
                    "foreign" => {
                        out.push(' ');
                        out.push_str(&foreign_placeholder(doc, e));
                        out.push(' ');
                    }
                    "ref" => out.push_str(&ref_markdown(doc, e)),
                    // Verschachtelte Tabelle in einer Zelle: flach statt
                    // Markdown-in-Markdown.
                    "table" => {
                        out.push(' ');
                        out.push_str(&doc.text_of(e).replace('\n', " "));
                        out.push(' ');
                    }
                    tag => {
                        write_inline(doc, e, out, notes);
                        // Nur Block-/Zellgrenzen trennen — Inline-Markup
                        // (b, i, sup, …) darf keine Wörter zerreissen (X18.5).
                        if matches!(
                            tag,
                            "num"
                                | "td"
                                | "th"
                                | "p"
                                | "paragraph"
                                | "heading"
                                | "item"
                                | "listIntroduction"
                                | "intro"
                                | "tr"
                                | "block"
                                | "content"
                        ) {
                            out.push(' ');
                        }
                    }
                }
            }
        }
    }
}

/// Whitespace-Normalisierung für Einzeiler.
fn collapse_inline(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Textknoten mit kollabiertem Whitespace anhängen: Zeilenumbrüche in
/// XML-Textknoten sind Pretty-Print-Artefakte, keine Blockgrenzen —
/// strukturelle Umbrüche setzt allein der Renderer. Wortgrenzen an den
/// Knoten-Rändern bleiben erhalten (ein Leerzeichen), Wortinneres wird
/// nie getrennt.
fn push_text_normalized(out: &mut String, t: &str) {
    let mut pending_ws = false;
    for ch in t.chars() {
        if ch.is_whitespace() {
            pending_ws = true;
        } else {
            if pending_ws {
                out.push(' ');
                pending_ws = false;
            }
            out.push(ch);
        }
    }
    if pending_ws {
        out.push(' ');
    }
}

/// Whitespace-Normalisierung für Chunk-Text: Soft-Hyphens raus (X18.3,
/// Tokenizer-Hygiene — doppelter Boden zur Parse-Normalisierung), Zeilen
/// intern kollabieren, Leerzeilen entfernen. Zeilenumbrüche bleiben
/// erhalten (Markdown-Tabellenzeilen!).
fn tidy_chunk(s: &str) -> String {
    let cleaned = s.replace('\u{00ad}', "");
    cleaned
        .lines()
        .map(collapse_inline)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// `<tr>`-Zeilen, die direkt zu dieser Tabelle gehören (verschachtelte
/// Tabellen ausgenommen).
fn table_rows(doc: &AknDocument, table: NodeId) -> Vec<NodeId> {
    doc.find_all(table, "tr")
        .into_iter()
        .filter(|&tr| {
            let mut cur = doc.parent(tr);
            while let Some(p) = cur {
                if doc.tag(p) == "table" {
                    return p == table;
                }
                cur = doc.parent(p);
            }
            false
        })
        .collect()
}

/// `true`, wenn zwischen `t` und `scope` (exklusiv) eine weitere Tabelle liegt.
fn has_table_ancestor_below(doc: &AknDocument, t: NodeId, scope: NodeId) -> bool {
    let mut cur = doc.parent(t);
    while let Some(p) = cur {
        if p == scope {
            return false;
        }
        if doc.tag(p) == "table" {
            return true;
        }
        cur = doc.parent(p);
    }
    false
}

/// Anzahl Zellen einer Zeile.
fn cell_count(doc: &AknDocument, tr: NodeId) -> usize {
    doc.children(tr)
        .filter(|&c| matches!(doc.tag(c), "td" | "th"))
        .count()
}

/// Eine Tabellenzeile als Markdown (`| a | b |`); zellenlose/leere Zeilen
/// entfallen. Pipes im Zelltext werden escaped.
fn markdown_row(doc: &AknDocument, tr: NodeId) -> Option<String> {
    let cells: Vec<String> = doc
        .children(tr)
        .filter(|&c| matches!(doc.tag(c), "td" | "th"))
        .map(|c| inline_markdown(doc, c).replace('|', "\\|"))
        .collect();
    if cells.is_empty() || cells.iter().all(String::is_empty) {
        return None;
    }
    Some(format!("| {} |", cells.join(" | ")))
}

/// Markdown-Separator (`| --- | --- |`) für `cols` Spalten.
fn markdown_separator(cols: usize) -> String {
    format!("|{}", " --- |".repeat(cols.max(1)))
}

/// Hängt eine Tabelle als Markdown-Zeilen an (`| a | b |`, Separator nach
/// echter `<th>`-Kopfzeile) — Tarife/Grenzwerte sind als Fliesstext
/// unauffindbar (21k Tabellen-Chunks, Strategie V3).
fn append_table_markdown(doc: &AknDocument, table: NodeId, out: &mut String) {
    for (i, &tr) in table_rows(doc, table).iter().enumerate() {
        let Some(row) = markdown_row(doc, tr) else {
            continue;
        };
        out.push_str(&row);
        out.push('\n');
        if i == 0 && doc.children(tr).any(|c| doc.tag(c) == "th") {
            out.push_str(&markdown_separator(cell_count(doc, tr)));
            out.push('\n');
        }
    }
}

/// Eine Tabelle als eigenständiger Markdown-Chunk-Text.
fn table_markdown(doc: &AknDocument, table: NodeId) -> String {
    let mut out = String::new();
    append_table_markdown(doc, table, &mut out);
    tidy_chunk(&out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testdoc::sample;

    #[test]
    fn hollowing_keeps_leaf_text_and_hollows_parents() {
        let doc = sample();
        let hollowed = hollow_document(&doc);
        let by_eid = |eid: &str| hollowed.iter().find(|h| h.eid == eid).unwrap();

        let art1 = by_eid("art_1");
        assert!(!art1.is_leaf);
        assert!(art1.text.starts_with("[Siehe Unterelemente:"));
        assert!(art1.text.contains("art_1/para_1"));
        assert!(art1.text.contains("art_1/para_2"));

        let para = by_eid("art_1/para_1");
        assert!(para.is_leaf);
        assert!(para.text.contains("Energieversorgung"));

        // Redundanz-Check (X20.2): Eltern tragen keinen eigenen Normtext mehr.
        let leaf_chars: usize = hollowed
            .iter()
            .filter(|h| h.is_leaf)
            .map(|h| h.text.chars().count())
            .sum();
        assert!(leaf_chars > 0);
    }

    #[test]
    fn chunks_structured_doc_per_article() {
        let doc = sample();
        let chunks = chunk_document(&doc).unwrap();
        // 3 Artikel, alle unter der Split-Schwelle.
        assert_eq!(chunks.len(), 3);
        let c = &chunks[0];
        assert_eq!(c.chunk_id, "eli/cc/2017/762#art_1");
        assert!(c.text.contains("Energieversorgung"));
        let m = &c.metadata;
        assert_eq!(m.sr.as_deref(), Some("730.0"));
        assert_eq!(m.title.as_deref(), Some("Energiegesetz"));
        assert_eq!(m.collection.as_deref(), Some("cc"));
        assert_eq!(m.language.as_deref(), Some("de"));
        assert_eq!(m.date.as_deref(), Some("2018-01-01"));
        assert_eq!(m.section_path, ["chap_1", "art_1"]);
        // Überschriften-Texte zum Pfad (S3-Embedding-Kontext, Strategie V3/V4):
        // nur ECHTE Eltern — die eigene Nummer/Überschrift steht im Chunk-Text
        // und würde als Kontext-Dopplung den Embedding-Input verwässern.
        assert_eq!(m.section_headings, ["1. Kapitel: Allgemeine Bestimmungen"]);
        assert_eq!(m.eid.as_deref(), Some("art_1"));
    }

    #[test]
    fn oversized_article_without_paragraphs_is_not_lost() {
        // Regressionstest: übergrosser Artikel ohne direkte paragraph-Kinder
        // muss als EIN Chunk erhalten bleiben, nicht stillschweigend wegfallen.
        let long = "Sehr langer Normtext. ".repeat(150); // > 2000 Zeichen
        let xml = format!(
            r#"<akomaNtoso xmlns="http://docs.oasis-open.org/legaldocml/ns/akn/3.0">
              <act name="publicLaw"><meta><identification>
                <FRBRWork><FRBRuri value="/eli/cc/2000/1"/></FRBRWork>
                <FRBRExpression><FRBRlanguage language="de"/></FRBRExpression>
              </identification></meta>
              <body><article eId="art_1"><num>Art. 1</num>
                <content><p>{long}</p></content>
              </article></body></act></akomaNtoso>"#
        );
        let doc = AknDocument::parse(&xml).unwrap();
        let chunks = chunk_document(&doc).unwrap();
        assert_eq!(chunks.len(), 1);
        assert!(chunks[0].text.chars().count() > 2000);
        assert_eq!(chunks[0].metadata.eid.as_deref(), Some("art_1"));
    }

    /// Baut einen Artikel mit Prosa und einer Tabelle mit `rows` Datenzeilen.
    fn doc_with_table(rows: usize) -> AknDocument {
        let mut table = String::from("<tr><th>Stoff</th><th>Grenzwert</th></tr>");
        for i in 0..rows {
            table.push_str(&format!(
                "<tr><td>Stoff Nummer {i} mit einem laengeren Namen</td><td>Grenzwert {i} \
                 Milligramm pro Kubikmeter Abluft</td></tr>"
            ));
        }
        let xml = format!(
            r#"<akomaNtoso xmlns="http://docs.oasis-open.org/legaldocml/ns/akn/3.0">
              <act name="publicLaw"><meta><identification>
                <FRBRWork><FRBRuri value="/eli/cc/2000/1"/></FRBRWork>
                <FRBRExpression><FRBRlanguage language="de"/></FRBRExpression>
              </identification></meta>
              <body><article eId="art_1"><num>Art. 1</num>
                <content><p>Die Grenzwerte richten sich nach folgender Tabelle.</p>
                <table eId="art_1/tbl_1">{table}</table></content>
              </article></body></act></akomaNtoso>"#
        );
        AknDocument::parse(&xml).unwrap()
    }

    #[test]
    fn small_table_stays_inside_article_chunk() {
        let chunks = chunk_document(&doc_with_table(3)).unwrap();
        assert_eq!(chunks.len(), 1);
        assert!(chunks[0].text.contains("Stoff Nummer 2"));
    }

    #[test]
    fn oversized_table_is_split_into_row_groups_with_repeated_header() {
        let chunks = chunk_document(&doc_with_table(120)).unwrap();
        // Prosa-Chunk plus mehrere Tabellen-Teile.
        assert!(
            chunks.len() > 2,
            "erwartet Prosa + Teile, war {}",
            chunks.len()
        );

        let prose = &chunks[0];
        assert_eq!(prose.chunk_id, "eli/cc/2000/1#art_1");
        assert!(prose.text.contains("Grenzwerte richten sich"));
        assert!(
            !prose.text.contains("Stoff Nummer"),
            "Tabelle gehoert nicht in den Prosa-Chunk"
        );

        // Jeder Teil traegt den wiederholten Markdown-Kopf (Kopfzeile +
        // Separator) und bleibt unter der Schwelle.
        let parts: Vec<&Chunk> = chunks[1..].iter().collect();
        for (i, part) in parts.iter().enumerate() {
            assert_eq!(
                part.chunk_id,
                format!("eli/cc/2000/1#art_1/tbl1/part{}", i + 1)
            );
            assert!(
                part.text
                    .starts_with("| Stoff | Grenzwert |\n| --- | --- |\n"),
                "Markdown-Kopf fehlt: {}",
                part.text
            );
            assert!(part.text.chars().count() <= SPLIT_THRESHOLD + 100);
            assert_eq!(part.metadata.eid.as_deref(), Some("art_1"));
        }

        // Keine Zeile geht verloren.
        let all: String = parts.iter().map(|c| c.text.as_str()).collect();
        for i in [0usize, 59, 119] {
            assert!(
                all.contains(&format!("Stoff Nummer {i}")),
                "Zeile {i} fehlt"
            );
        }
    }

    #[test]
    fn chunk_ids_use_undated_work_eli() {
        // Datierte Konsolidierungs-URI (Live-Form) darf NICHT in die Chunk-ID.
        let xml = r#"<akomaNtoso xmlns="http://docs.oasis-open.org/legaldocml/ns/akn/3.0">
          <act name="publicLaw"><meta><identification>
            <FRBRWork><FRBRuri value="https://fedlex.data.admin.ch/eli/cc/2017/762/20260401"/></FRBRWork>
            <FRBRExpression><FRBRlanguage language="de"/></FRBRExpression>
          </identification></meta>
          <body><article eId="art_1"><num>Art. 1</num>
            <content><p>Kurzer Text.</p></content>
          </article></body></act></akomaNtoso>"#;
        let doc = AknDocument::parse(xml).unwrap();
        let chunks = chunk_document(&doc).unwrap();
        assert_eq!(chunks[0].chunk_id, "eli/cc/2017/762#art_1");
        assert_eq!(chunks[0].metadata.eli.as_deref(), Some("eli/cc/2017/762"));
        assert_eq!(chunks[0].metadata.collection.as_deref(), Some("cc"));
    }

    // ─────────── Chunk-Text-Anreicherungen (SOTA-T13, Strategie V3) ───────────

    /// Hilfsfunktion: Chunk zu einer eId.
    fn chunk_for<'a>(chunks: &'a [Chunk], eid: &str) -> &'a Chunk {
        chunks
            .iter()
            .find(|c| c.metadata.eid.as_deref() == Some(eid))
            .unwrap_or_else(|| panic!("kein Chunk fuer {eid}"))
    }

    #[test]
    fn tables_render_as_markdown_rows() {
        let chunks = chunk_document(&sample()).unwrap();
        let art2 = chunk_for(&chunks, "art_2");
        // Tarife/Grenzwerte sind als Fliesstext unauffindbar (Strategie V3).
        assert!(art2.text.contains("| Jahr | GWh |"), "got: {}", art2.text);
        assert!(art2.text.contains("| --- | --- |"));
        assert!(art2.text.contains("| 2035 | 37400 |"));
    }

    #[test]
    fn footnotes_survive_as_compact_history_markers() {
        let chunks = chunk_document(&sample()).unwrap();
        let art1 = chunk_for(&chunks, "art_1");
        // Wortinnere Fussnote (X6.4): Das Normwort bleibt intakt, die
        // Historie wandert kompakt ans Blockende.
        assert!(
            art1.text.contains("Energieverbrauch"),
            "Fussnote zerreisst das Wort: {}",
            art1.text
        );
        assert!(
            art1.text
                .contains("[Historie: Fassung gemäss Ziff. I des BG vom 21. Juni 2019"),
            "Historie fehlt: {}",
            art1.text
        );
        // AS-Verweis in der Historie als ELI-Link (Cross-Law-Kandidat).
        assert!(
            art1.text
                .contains("[AS 2020 752](https://fedlex.data.admin.ch/eli/oc/2020/752)"),
            "ELI-Link in Historie fehlt: {}",
            art1.text
        );
    }

    #[test]
    fn eli_refs_become_links_other_hrefs_stay_text() {
        let xml = r#"<akomaNtoso xmlns="http://docs.oasis-open.org/legaldocml/ns/akn/3.0">
          <act name="publicLaw"><meta><identification>
            <FRBRWork><FRBRuri value="/eli/cc/2000/1"/></FRBRWork>
            <FRBRExpression><FRBRlanguage language="de"/></FRBRExpression>
          </identification></meta>
          <body><article eId="art_1"><num>Art. 1</num>
            <content><p>Gestützt auf <ref href="https://fedlex.data.admin.ch/eli/cc/1999/404">Art. 89 BV</ref>,
            <ref href="https://www.admin.ch/gov/de/start.html">die Botschaft</ref>
            und <ref>Art. 1</ref>.</p></content>
          </article></body></act></akomaNtoso>"#;
        let doc = AknDocument::parse(xml).unwrap();
        let chunks = chunk_document(&doc).unwrap();
        let text = &chunks[0].text;
        // ELI-href → Markdown-Link.
        assert!(
            text.contains("[Art. 89 BV](https://fedlex.data.admin.ch/eli/cc/1999/404)"),
            "got: {text}"
        );
        // Nicht-ELI-href → nur Text, keine URL im Chunk.
        assert!(text.contains("die Botschaft"));
        assert!(!text.contains("www.admin.ch"), "Fremd-URL leckt: {text}");
        // href-los (X11.2) → nackter Text.
        assert!(text.contains("und Art. 1."));
    }

    #[test]
    fn soft_hyphens_removed_and_whitespace_collapsed() {
        // Soft-Hyphen (X18.3) und Pretty-Print-Umbrüche im Textknoten dürfen
        // den Chunk-Text nicht verrauschen.
        let xml = "<akomaNtoso xmlns=\"http://docs.oasis-open.org/legaldocml/ns/akn/3.0\">\
          <act name=\"publicLaw\"><meta><identification>\
            <FRBRWork><FRBRuri value=\"/eli/cc/2000/1\"/></FRBRWork>\
            <FRBRExpression><FRBRlanguage language=\"de\"/></FRBRExpression>\
          </identification></meta>\
          <body><article eId=\"art_1\"><num>Art. 1</num>\
            <content><p>Die Ener\u{ad}gie   ist\n      wichtig.</p></content>\
          </article></body></act></akomaNtoso>";
        let doc = AknDocument::parse(xml).unwrap();
        let chunks = chunk_document(&doc).unwrap();
        assert_eq!(chunks[0].text, "Art. 1 Die Energie ist wichtig.");
    }

    #[test]
    fn foreign_becomes_alt_text_or_placeholder() {
        // Ohne Alt-Text: MathML → [Formel] (Fixture art_2, X18.4).
        let chunks = chunk_document(&sample()).unwrap();
        let art2 = chunk_for(&chunks, "art_2");
        assert!(art2.text.contains("[Formel]"), "got: {}", art2.text);
        assert!(
            !art2.text.contains("mrow"),
            "MathML-Rauschen: {}",
            art2.text
        );

        // Mit @alttext gewinnt der Alt-Text; SVG ohne Titel → [Grafik].
        let xml = r#"<akomaNtoso xmlns="http://docs.oasis-open.org/legaldocml/ns/akn/3.0">
          <act name="publicLaw"><meta><identification>
            <FRBRWork><FRBRuri value="/eli/cc/2000/1"/></FRBRWork>
            <FRBRExpression><FRBRlanguage language="de"/></FRBRExpression>
          </identification></meta>
          <body><article eId="art_1"><num>Art. 1</num>
            <content><p>Leistung:</p>
            <foreign><math alttext="P = 2 kW"><mrow><mi>P</mi></mrow></math></foreign>
            <foreign><svg><path d="M0 0"/></svg></foreign></content>
          </article></body></act></akomaNtoso>"#;
        let doc = AknDocument::parse(xml).unwrap();
        let chunks = chunk_document(&doc).unwrap();
        let text = &chunks[0].text;
        assert!(text.contains("P = 2 kW"), "Alt-Text fehlt: {text}");
        assert!(!text.contains("[Formel]"), "Platzhalter trotz Alt-Text");
        assert!(text.contains("[Grafik]"), "SVG-Platzhalter fehlt: {text}");
    }

    #[test]
    fn component_chunks_carry_annex_and_parent_context() {
        // Anhänge sind eigene FRBR-Werke (X19.8) — ohne Kontext-Kopf wüsste
        // das Retrieval nicht, zu welchem Erlass ein Anhang-Chunk gehört.
        let xml = r#"<akomaNtoso xmlns="http://docs.oasis-open.org/legaldocml/ns/akn/3.0">
          <act name="publicLaw"><meta><identification>
            <FRBRWork><FRBRuri value="/eli/cc/2010/5"/>
              <FRBRnumber value="SR 814.01"/>
              <FRBRname xml:lang="de" value="Umweltschutzgesetz"/></FRBRWork>
            <FRBRExpression><FRBRlanguage language="de"/></FRBRExpression>
          </identification></meta>
          <preface><p><docTitle>USG</docTitle></p></preface>
          <components><component eId="cmp_1"><doc name="annex">
            <meta><identification>
              <FRBRWork><FRBRuri value="/eli/cc/2010/5/anx_2"/><FRBRname value="Anhang 2"/></FRBRWork>
              <FRBRExpression><FRBRlanguage language="de"/></FRBRExpression>
            </identification></meta>
            <mainBody><level eId="anx_2/lvl_1"><content><p>Grenzwerte für Feinstaub
              und weitere Schadstoffe, genügend lang, um kein Verweis-Stub zu sein —
              mit etwas zusätzlichem Text zur Sicherheit.</p></content></level></mainBody>
          </doc></component></components></act></akomaNtoso>"#;
        let doc = AknDocument::parse(xml).unwrap();
        let chunks = chunk_document(&doc).unwrap();
        assert_eq!(chunks.len(), 1);
        let c = &chunks[0];
        // Chunk-ID bleibt auf dem Anhang-Werk (eigene ELI).
        assert_eq!(c.chunk_id, "eli/cc/2010/5/anx_2#anx_2/lvl_1");
        // Kontext-Kopf: Anhang-Titel + Bezugserlass (Strategie V3).
        assert!(
            c.text
                .starts_with("[Anhang 2 — Anhang zu: Umweltschutzgesetz (SR 814.01)]\n"),
            "Kontext-Kopf fehlt: {}",
            c.text
        );
        assert!(c.text.contains("Grenzwerte für Feinstaub"));
    }

    /// Zitierfähigkeits-Wächter (harte Leitplanke, SOTA-T13): Die
    /// V3-Anreicherungen gelten NUR für den Chunk-Text — die Reader-Sicht
    /// (`get_element_text`, TXT-02) liefert weiterhin den authentischen
    /// Normtext, Fussnoten getrennt, Tabellen als Rohtext.
    #[test]
    fn chunk_enrichment_never_leaks_into_reader_text() {
        use crate::testdoc::stichtag;
        use crate::text::get_element_text;

        let doc = sample();

        // Reader: Tabelle als Rohtext, keine Markdown-/Historie-Artefakte.
        let reader = get_element_text(&doc, "art_2", stichtag()).unwrap();
        let norm = &reader.data().text;
        assert!(
            norm.contains("Jahr") && norm.contains("37400"),
            "Rohtext-Tabelle fehlt: {norm}"
        );
        assert!(!norm.contains('|'), "Markdown leckt in Normtext: {norm}");
        assert!(!norm.contains("[Formel]"), "Platzhalter leckt: {norm}");
        assert!(!norm.contains("]("), "Markdown-Link leckt: {norm}");

        // Reader: Fussnote getrennt (notes[]), nie inline.
        let reader = get_element_text(&doc, "art_1", stichtag()).unwrap();
        assert!(!reader.data().text.contains("[Historie:"));
        assert!(!reader.data().text.contains("AS 2020 752"));
        assert_eq!(reader.data().notes.len(), 1);

        // Chunker: dieselben Elemente angereichert.
        let chunks = chunk_document(&doc).unwrap();
        assert!(chunk_for(&chunks, "art_2").text.contains("| Jahr | GWh |"));
        assert!(chunk_for(&chunks, "art_1").text.contains("[Historie:"));
    }
}
