//! The rmcp tool router: the v0 spine plus the navigator surface
//! (TOOLSET-v1.md — thirty-five tools since BT), dot-notation capability ids,
//! results as pretty JSON text; domain refusals (typed error objects)
//! surface as MCP tool errors carrying the typed JSON — machines
//! branch on `error`, never on prose.
//!
//! **Stage-one lines (E16 two-stage discovery).** Every description
//! below is the ONE line a model sees in stage one — at the gateway
//! (`meta.tools`) and here (`tools/list`). House rule, checked by the
//! e2e suite: at most 160 characters, begins with the verb, says WHEN
//! to use the tool, ends with whether the answer is a `hint` or a
//! `norm`, and carries the trigger words a question would contain
//! (SR, Artikel, Absatz, Anhang, Fassung, Verweis, Änderung).

use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock};
use rmcp::{tool, tool_handler, tool_router, ErrorData, ServerHandler};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::domain::{self, Ctx};

pub struct FedlexServer {
    pub ctx: std::sync::Arc<Ctx>,
}

impl Clone for FedlexServer {
    fn clone(&self) -> Self {
        Self {
            ctx: self.ctx.clone(),
        }
    }
}

fn emit(result: anyhow::Result<serde_json::Value>) -> Result<CallToolResult, ErrorData> {
    match result {
        Ok(value) => {
            let text = serde_json::to_string_pretty(&value)
                .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
            if value.get("error").is_some() {
                Ok(CallToolResult::error(vec![ContentBlock::text(text)]))
            } else {
                Ok(CallToolResult::success(vec![ContentBlock::text(text)]))
            }
        }
        Err(e) => Ok(CallToolResult::error(vec![ContentBlock::text(format!(
            "{{\"error\":\"internal\",\"detail\":\"{e:#}\"}}"
        ))])),
    }
}

/// The house rule for a stage-one line, as a checkable predicate:
/// ≤ 160 characters, starts with an upper-case verb, ends with the
/// answer kind. Shared with the e2e suite.
pub fn summary_conforms(summary: &str) -> Result<(), String> {
    let n = summary.chars().count();
    if n > 160 {
        return Err(format!("{n} characters, the stage-one limit is 160"));
    }
    if !summary.chars().next().is_some_and(char::is_uppercase) {
        return Err("must begin with the verb, capitalised".into());
    }
    if !(summary.ends_with(" norm.") || summary.ends_with(" hint.")) {
        return Err("must end with «norm.» or «hint.»".into());
    }
    if !summary.contains("use ") {
        return Err("must say WHEN to use the tool («use when/for/to/before …»)".into());
    }
    Ok(())
}

// ---------------------------------------------------------------------
// Parameter shapes (shared with the gateway mount)
// Closed against unknown keys (`deny_unknown_fields`) with the same
// reason as the lindas surface: a near-miss argument name was accepted
// and silently dropped, and the caller was answered as if it had never
// asked (audit of 01.09.2026). A stray key is a typed refusal now.
// ---------------------------------------------------------------------

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SrParams {
    /// Systematic (SR) number, e.g. «832.10».
    pub sr: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SearchParams {
    pub query: String,
    pub limit: Option<u32>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EliParams {
    /// Fedlex ELI of the consolidation abstract.
    pub eli: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EliAsOfOptParams {
    pub eli: String,
    /// Optional ISO date; absent = today, echoed back resolved.
    pub as_of: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EliAsOfParams {
    pub eli: String,
    /// ISO date YYYY-MM-DD.
    pub as_of: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CitationParams {
    pub eli: String,
    /// «in» | «out» (the foreseen-impact graph) or «cites» | «cited_by»
    /// (the formal citation graph, act level).
    pub direction: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReadArticleParams {
    /// Dated consolidation: `<abstract-eli>/<YYYYMMDD>`.
    pub eli_version: String,
    /// Akoma-Ntoso eId, e.g. «art_10a», or a path eId such as
    /// «art_2/para_1» or «annex_u1/lvl_u1».
    pub eid: String,
    /// Manifestation language (de|fr|it|en|rm); default de. Which
    /// languages a version carries as XML is the graph's answer —
    /// fedlex.list_expressions shows it before a read.
    pub lang: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VersionParams {
    /// Dated consolidation: `<abstract-eli>/<YYYYMMDD>`.
    pub eli_version: String,
    /// Manifestation language (de|fr|it|en|rm); default de. Which
    /// languages a version carries as XML is the graph's answer —
    /// fedlex.list_expressions shows it before a read.
    pub lang: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StructureParams {
    /// Dated consolidation: `<abstract-eli>/<YYYYMMDD>`.
    pub eli_version: String,
    /// Manifestation language (de|fr|it|en|rm); default de. Which
    /// languages a version carries as XML is the graph's answer —
    /// fedlex.list_expressions shows it before a read.
    pub lang: Option<String>,
    /// «article» (default: the skeleton down to articles) or «full»
    /// (the whole tree down to paragraphs and items).
    pub depth: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SearchTextParams {
    /// Dated consolidation: `<abstract-eli>/<YYYYMMDD>`.
    pub eli_version: String,
    /// Word or phrase; case-insensitive substring.
    pub query: String,
    /// Manifestation language (de|fr|it|en|rm); default de. Which
    /// languages a version carries as XML is the graph's answer —
    /// fedlex.list_expressions shows it before a read.
    pub lang: Option<String>,
    /// Max hits (default 20, at most 100); `total` counts beyond it.
    pub limit: Option<u32>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReadDocumentParams {
    /// Dated consolidation: `<abstract-eli>/<YYYYMMDD>`.
    pub eli_version: String,
    /// Manifestation language (de|fr|it|en|rm); default de. Which
    /// languages a version carries as XML is the graph's answer —
    /// fedlex.list_expressions shows it before a read.
    pub lang: Option<String>,
    /// Character budget (default 120 000, at most 400 000).
    pub max_chars: Option<u32>,
    /// Continuation: the `next_offset` of the previous answer.
    pub offset: Option<u32>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReferencesParams {
    /// Dated consolidation: `<abstract-eli>/<YYYYMMDD>`.
    pub eli_version: String,
    /// Optional scope: only references made from this eId or below.
    pub eid: Option<String>,
    /// Manifestation language (de|fr|it|en|rm); default de. Which
    /// languages a version carries as XML is the graph's answer —
    /// fedlex.list_expressions shows it before a read.
    pub lang: Option<String>,
    /// Page size (default 200, at most 1000).
    pub limit: Option<u32>,
    /// Continuation: the `next_offset` of the previous answer.
    pub offset: Option<u32>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ModificationsParams {
    /// Dated consolidation: `<abstract-eli>/<YYYYMMDD>`.
    pub eli_version: String,
    /// Optional scope: only notes anchored at this eId or below.
    pub eid: Option<String>,
    /// Manifestation language (de|fr|it|en|rm); default de. Which
    /// languages a version carries as XML is the graph's answer —
    /// fedlex.list_expressions shows it before a read.
    pub lang: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EliEidParams {
    /// Fedlex ELI of the consolidation abstract.
    pub eli: String,
    /// Akoma-Ntoso eId of the element, e.g. «art_14a» or «art_2/para_1».
    pub eid: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VersionOnlyParams {
    /// Dated consolidation: `<abstract-eli>/<YYYYMMDD>`.
    pub eli_version: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VocabularyParams {
    /// Scheme id: «enforcement-status», «subdivision-type»,
    /// «legal-taxonomy», «impact-type», «resource-type», «language», …
    pub vocabulary: String,
    /// A label fragment (case-insensitive, any language) or a
    /// vocabulary IRI to decode.
    pub query: String,
    /// Label language (de|fr|it|en|rm); default de.
    pub lang: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RelatedParams {
    /// Fedlex ELI of the act — or give `sr`.
    pub eli: Option<String>,
    /// SR number as an alternative entry, e.g. «832.10».
    pub sr: Option<String>,
    /// Max candidates (default 20, at most 50).
    pub limit: Option<u32>,
}

// ---- BR wave 2 parameter shapes ----

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TablesParams {
    /// Dated consolidation: `<abstract-eli>/<YYYYMMDD>`.
    pub eli_version: String,
    /// Optional scope: an eId (an annex level such as «annex_3/lvl_u1/lvl_2», an article).
    pub eid: Option<String>,
    /// Manifestation language (de|fr|it|en|rm); default de. Which
    /// languages a version carries as XML is the graph's answer —
    /// fedlex.list_expressions shows it before a read.
    pub lang: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct QuoteParams {
    /// Dated consolidation: `<abstract-eli>/<YYYYMMDD>` — the version that was read.
    pub eli_version: String,
    /// The element that was read: an eId such as «art_6» or a path eId
    /// such as «art_6/para_1» or «annex_3/lvl_u1/lvl_2».
    pub eid: String,
    /// The wording to check, verbatim; «…» or «[...]» marks an omission
    /// (each part must occur, in order).
    pub quote: String,
    /// Manifestation language (de|fr|it|en|rm); default de. Which
    /// languages a version carries as XML is the graph's answer —
    /// fedlex.list_expressions shows it before a read.
    pub lang: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CiteParams {
    /// Dated consolidation: `<abstract-eli>/<YYYYMMDD>`.
    pub eli_version: String,
    /// The place to label: an eId such as «art_7/para_1/lbl_b», «art_23_a»
    /// or «annex_3/lvl_u1».
    pub eid: String,
    /// Label language (de|fr|it|en|rm) — the manifestation read and the
    /// abbreviation used; default de.
    pub lang: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReferenceParams {
    /// A citation in plain text, e.g. «Art. 7 Abs. 1 lit. b LSV», «Anhang 3
    /// Ziff. 2 LSV», «Art. 8 EMRK i.V.m. Art. 36 BV», «SR 832.10».
    pub text: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CompareParams {
    /// Fedlex ELI of the act (consolidation abstract).
    pub eli: String,
    /// The older consolidation: a version IRI or a date (YYYY-MM-DD / YYYYMMDD).
    pub from_version: String,
    /// The newer consolidation: a version IRI or a date.
    pub to_version: String,
    /// Optional scope: one element (article or path eId); default every article.
    pub eid: Option<String>,
    /// Manifestation language (de|fr|it|en|rm); default de. Which
    /// languages a version carries as XML is the graph's answer —
    /// fedlex.list_expressions shows it before a read.
    pub lang: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NodeParams {
    /// Any Fedlex IRI (https://fedlex.data.admin.ch/…).
    pub iri: String,
    /// Edges per direction (default 20, at most 50).
    pub limit: Option<u32>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TreatySearchParams {
    /// A word of the treaty title (case-insensitive).
    pub query: Option<String>,
    /// Partner country as a Fedlex country vocabulary IRI.
    pub country: Option<String>,
    /// true = bilateral only, false = multilateral only.
    pub bilateral: Option<bool>,
    /// Max hits (default 20, at most 50).
    pub limit: Option<u32>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TreatyInfoParams {
    /// The treaty process IRI (https://fedlex.data.admin.ch/eli/treaty/…), from find_treaties.
    pub eli: String,
    /// Preferred title language (de|fr|it|en|rm); default de.
    pub lang: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConsultationsParams {
    /// Fedlex ELI of the act — its drafts are resolved first (at most five).
    pub eli: Option<String>,
    /// Or a draft IRI (from get_drafts).
    pub draft: Option<String>,
    /// Optional status filter: the consultation-status IRI or its last segment.
    pub status: Option<String>,
    /// Max consultations (default 20, at most 50).
    pub limit: Option<u32>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConsultationDocsParams {
    /// The consultation IRI (from get_consultations).
    pub consultation: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MemorialParams {
    /// The AS publication ELI (https://fedlex.data.admin.ch/eli/oc/…), from get_oc_act.
    pub eli: String,
    /// Max acts of the issue (default 20, at most 50).
    pub limit: Option<u32>,
}

#[tool_router(vis = "pub")]
impl FedlexServer {
    #[tool(
        name = "fedlex.resolve_sr",
        description = "Resolve an SR number (e.g. 832.10) to the act's ELI, titles and in-force status: use when a question names an SR; predecessors stay visible. norm.",
        annotations(read_only_hint = true)
    )]
    pub async fn resolve_sr(
        &self,
        Parameters(p): Parameters<SrParams>,
    ) -> Result<CallToolResult, ErrorData> {
        emit(domain::resolve_sr(&self.ctx, &p.sr))
    }

    #[tool(
        name = "fedlex.search_law",
        description = "Search acts by title keyword or official abbreviation (KVG, StPO, OR): use when you know the name but not the SR or ELI; in-force acts rank first. hint.",
        annotations(read_only_hint = true)
    )]
    pub async fn search_law(
        &self,
        Parameters(p): Parameters<SearchParams>,
    ) -> Result<CallToolResult, ErrorData> {
        emit(domain::search_law(&self.ctx, &p.query, p.limit))
    }

    #[tool(
        name = "fedlex.get_law_metadata",
        description = "Read the JOLux profile of an act (titles de/fr/it, status, dates, identifier) for an ELI: use to confirm a search hit before you cite. norm.",
        annotations(read_only_hint = true)
    )]
    pub async fn get_law_metadata(
        &self,
        Parameters(p): Parameters<EliAsOfOptParams>,
    ) -> Result<CallToolResult, ErrorData> {
        emit(domain::get_law_metadata(
            &self.ctx,
            &p.eli,
            p.as_of.as_deref(),
        ))
    }

    #[tool(
        name = "fedlex.list_versions",
        description = "List every dated consolidation (Fassung) of an act, future ones included: use to pick the eli_version the reading tools need. norm.",
        annotations(read_only_hint = true)
    )]
    pub async fn list_versions(
        &self,
        Parameters(p): Parameters<EliParams>,
    ) -> Result<CallToolResult, ErrorData> {
        emit(domain::list_versions(&self.ctx, &p.eli))
    }

    #[tool(
        name = "fedlex.resolve_consolidation_at",
        description = "Resolve which consolidation (Fassung) of an act governed on an ISO date: use before reading text for a past or future Stichtag. norm.",
        annotations(read_only_hint = true)
    )]
    pub async fn resolve_consolidation_at(
        &self,
        Parameters(p): Parameters<EliAsOfParams>,
    ) -> Result<CallToolResult, ErrorData> {
        emit(domain::resolve_consolidation_at(
            &self.ctx, &p.eli, &p.as_of,
        ))
    }

    #[tool(
        name = "fedlex.check_in_force",
        description = "Check whether an act was in force on a date; false is a valid answer, never an error: use for «gilt das noch?» questions. norm.",
        annotations(read_only_hint = true)
    )]
    pub async fn check_in_force(
        &self,
        Parameters(p): Parameters<EliAsOfParams>,
    ) -> Result<CallToolResult, ErrorData> {
        emit(domain::check_in_force(&self.ctx, &p.eli, &p.as_of))
    }

    #[tool(
        name = "fedlex.get_citations",
        description = "List an act's relations: cites|cited_by (formal citations, act level) or in|out (foreseen impacts, mostly consultation drafts): use to see who cites X. norm.",
        annotations(read_only_hint = true)
    )]
    pub async fn get_citations(
        &self,
        Parameters(p): Parameters<CitationParams>,
    ) -> Result<CallToolResult, ErrorData> {
        emit(domain::get_citations(&self.ctx, &p.eli, &p.direction))
    }

    #[tool(
        name = "fedlex.read_article",
        description = "Read one element (Artikel, Absatz, Anhang) of a dated consolidation by eId, e.g. art_6 or annex_u1/lvl_u1: use to quote a norm. norm.",
        annotations(read_only_hint = true)
    )]
    pub async fn read_article(
        &self,
        Parameters(p): Parameters<ReadArticleParams>,
    ) -> Result<CallToolResult, ErrorData> {
        emit(domain::read_article(
            &self.ctx,
            &p.eli_version,
            &p.eid,
            p.lang.as_deref(),
        ))
    }

    // ---- BQ wave 1, A: XML tools ------------------------------------

    #[tool(
        name = "fedlex.get_structure",
        description = "Outline one consolidation (sections, articles with eId, num and heading): use when you know the act but not the article number. norm.",
        annotations(read_only_hint = true)
    )]
    pub async fn get_structure(
        &self,
        Parameters(p): Parameters<StructureParams>,
    ) -> Result<CallToolResult, ErrorData> {
        emit(domain::get_structure(
            &self.ctx,
            &p.eli_version,
            p.lang.as_deref(),
            p.depth.as_deref(),
        ))
    }

    #[tool(
        name = "fedlex.search_text",
        description = "Find where a word occurs inside ONE consolidation (hits with eId and Artikel): use before read_article when the article is unknown. hint.",
        annotations(read_only_hint = true)
    )]
    pub async fn search_text(
        &self,
        Parameters(p): Parameters<SearchTextParams>,
    ) -> Result<CallToolResult, ErrorData> {
        emit(domain::search_text(
            &self.ctx,
            &p.eli_version,
            &p.query,
            p.lang.as_deref(),
            p.limit,
        ))
    }

    #[tool(
        name = "fedlex.read_document",
        description = "Read a whole small act or Verordnung as capped Markdown (truncated flag, continuation offset): use for short acts; quote via read_article. norm.",
        annotations(read_only_hint = true)
    )]
    pub async fn read_document(
        &self,
        Parameters(p): Parameters<ReadDocumentParams>,
    ) -> Result<CallToolResult, ErrorData> {
        emit(domain::read_document(
            &self.ctx,
            &p.eli_version,
            p.lang.as_deref(),
            p.max_chars,
            p.offset,
        ))
    }

    #[tool(
        name = "fedlex.get_references",
        description = "List the references (Verweise) an act's text makes, with ELI where linked, optionally within one eId: use to follow cross-references. hint.",
        annotations(read_only_hint = true)
    )]
    pub async fn get_references(
        &self,
        Parameters(p): Parameters<ReferencesParams>,
    ) -> Result<CallToolResult, ErrorData> {
        emit(domain::get_references(
            &self.ctx,
            &p.eli_version,
            p.eid.as_deref(),
            p.lang.as_deref(),
            p.limit,
            p.offset,
        ))
    }

    #[tool(
        name = "fedlex.get_modifications",
        description = "List the amendment notes («Fassung gemäss …», AS refs) per element of a consolidation: use for «wann und wodurch wurde Art. X geändert?». norm.",
        annotations(read_only_hint = true)
    )]
    pub async fn get_modifications(
        &self,
        Parameters(p): Parameters<ModificationsParams>,
    ) -> Result<CallToolResult, ErrorData> {
        emit(domain::get_modifications(
            &self.ctx,
            &p.eli_version,
            p.eid.as_deref(),
            p.lang.as_deref(),
        ))
    }

    #[tool(
        name = "fedlex.list_annexes",
        description = "List the annexes (Anhänge) of a consolidation with titles and path eIds (annex_u1/…): use before reading an Anhang with read_article. norm.",
        annotations(read_only_hint = true)
    )]
    pub async fn list_annexes(
        &self,
        Parameters(p): Parameters<VersionParams>,
    ) -> Result<CallToolResult, ErrorData> {
        emit(domain::list_annexes(
            &self.ctx,
            &p.eli_version,
            p.lang.as_deref(),
        ))
    }

    // ---- BQ wave 1, B: JOLux tools ----------------------------------

    #[tool(
        name = "fedlex.get_article_history",
        description = "Trace which amendments and consolidations changed one Artikel (eId) of an act, with dates: use for «seit wann gilt Art. X so?». norm.",
        annotations(read_only_hint = true)
    )]
    pub async fn get_article_history(
        &self,
        Parameters(p): Parameters<EliEidParams>,
    ) -> Result<CallToolResult, ErrorData> {
        emit(domain::get_article_history(&self.ctx, &p.eli, &p.eid))
    }

    #[tool(
        name = "fedlex.get_subdivisions",
        description = "List the subdivisions JOLux knows for an act (amended elements only, a gap catalogue): use to see which Artikel carry amendments; outline: get_structure. norm.",
        annotations(read_only_hint = true)
    )]
    pub async fn get_subdivisions(
        &self,
        Parameters(p): Parameters<EliParams>,
    ) -> Result<CallToolResult, ErrorData> {
        emit(domain::get_subdivisions(&self.ctx, &p.eli))
    }

    #[tool(
        name = "fedlex.get_taxonomy",
        description = "Classify an act in the systematic collection (SR branch chain, notation, labels de/fr/it): use for «zu welchem Rechtsgebiet gehört X?». norm.",
        annotations(read_only_hint = true)
    )]
    pub async fn get_taxonomy(
        &self,
        Parameters(p): Parameters<EliParams>,
    ) -> Result<CallToolResult, ErrorData> {
        emit(domain::get_taxonomy(&self.ctx, &p.eli))
    }

    #[tool(
        name = "fedlex.list_expressions",
        description = "List the language versions and manifestations (XML, PDF) of one consolidation: use before reading to see whether a Fassung is PDF-only. norm.",
        annotations(read_only_hint = true)
    )]
    pub async fn list_expressions(
        &self,
        Parameters(p): Parameters<VersionOnlyParams>,
    ) -> Result<CallToolResult, ErrorData> {
        emit(domain::list_expressions(&self.ctx, &p.eli_version))
    }

    #[tool(
        name = "fedlex.resolve_vocabulary_label",
        description = "Look up a Fedlex vocabulary term (enforcement-status, language, …) by label or IRI: use to decode a coded value from another answer. hint.",
        annotations(read_only_hint = true)
    )]
    pub async fn resolve_vocabulary_label(
        &self,
        Parameters(p): Parameters<VocabularyParams>,
    ) -> Result<CallToolResult, ErrorData> {
        emit(domain::resolve_vocabulary_label(
            &self.ctx,
            &p.vocabulary,
            &p.query,
            p.lang.as_deref(),
        ))
    }

    #[tool(
        name = "fedlex.find_related_topic",
        description = "Find acts in the same field of law via the legal taxonomy, by ELI or SR: use to discover neighbouring Erlasse; candidates only. hint.",
        annotations(read_only_hint = true)
    )]
    pub async fn find_related_topic(
        &self,
        Parameters(p): Parameters<RelatedParams>,
    ) -> Result<CallToolResult, ErrorData> {
        emit(domain::find_related_topic(
            &self.ctx,
            p.eli.as_deref(),
            p.sr.as_deref(),
            p.limit,
        ))
    }

    // ---- BR wave 2: research-critical tools and the holdings beyond the SR ----

    #[tool(
        name = "fedlex.extract_tables",
        description = "Extract the tables of a consolidation or of one element (annex limit values, tariffs) as header and rows: use when a norm is a table, not prose. norm.",
        annotations(read_only_hint = true)
    )]
    pub async fn extract_tables(
        &self,
        Parameters(p): Parameters<TablesParams>,
    ) -> Result<CallToolResult, ErrorData> {
        emit(domain::extract_tables(
            &self.ctx,
            &p.eli_version,
            p.eid.as_deref(),
            p.lang.as_deref(),
        ))
    }

    #[tool(
        name = "fedlex.parse_reference",
        description = "Parse a citation («Art. 7 Abs. 1 lit. b LSV») into act, article eId and path proposal: use to turn a quoted Fundstelle into what read_article can open. hint.",
        annotations(read_only_hint = true)
    )]
    pub async fn parse_reference(
        &self,
        Parameters(p): Parameters<ReferenceParams>,
    ) -> Result<CallToolResult, ErrorData> {
        emit(domain::parse_reference(&self.ctx, &p.text))
    }

    #[tool(
        name = "fedlex.check_quote",
        description = "Check a quote (Zitat) against the norm text of one element: use before citing, to prove the wording is in what was read (Belegkette). norm.",
        annotations(read_only_hint = true)
    )]
    pub async fn check_quote(
        &self,
        Parameters(p): Parameters<QuoteParams>,
    ) -> Result<CallToolResult, ErrorData> {
        emit(domain::check_quote(
            &self.ctx,
            &p.eli_version,
            &p.eid,
            &p.quote,
            p.lang.as_deref(),
        ))
    }

    #[tool(
        name = "fedlex.cite",
        description = "Cite an element as its canonical Fundstelle («Art. 7 Abs. 1 Bst. b LSV») from eli_version and eId: use to label a read place in the Belegkette. norm.",
        annotations(read_only_hint = true)
    )]
    pub async fn cite(
        &self,
        Parameters(p): Parameters<CiteParams>,
    ) -> Result<CallToolResult, ErrorData> {
        emit(domain::cite(
            &self.ctx,
            &p.eli_version,
            &p.eid,
            p.lang.as_deref(),
        ))
    }

    #[tool(
        name = "fedlex.compare_versions",
        description = "Compare an element or every article between two Fassungen of an act — added, removed, changed paragraphs with wording: use for «was hat sich geändert?». norm.",
        annotations(read_only_hint = true)
    )]
    pub async fn compare_versions(
        &self,
        Parameters(p): Parameters<CompareParams>,
    ) -> Result<CallToolResult, ErrorData> {
        emit(domain::compare_versions(
            &self.ctx,
            &p.eli,
            &p.from_version,
            &p.to_version,
            p.eid.as_deref(),
            p.lang.as_deref(),
        ))
    }

    #[tool(
        name = "fedlex.explore_node",
        description = "Explore a JOLux node's edges (predicates and neighbours, both directions, capped): use to debug what the graph holds about an IRI; never as proof. hint.",
        annotations(read_only_hint = true)
    )]
    pub async fn explore_node(
        &self,
        Parameters(p): Parameters<NodeParams>,
    ) -> Result<CallToolResult, ErrorData> {
        emit(domain::explore_node(&self.ctx, &p.iri, p.limit))
    }

    #[tool(
        name = "fedlex.detect_foreign_content",
        description = "Detect what the text tools hide in a Fassung: sections in another language (xml:lang) and <foreign> islands (formulas, graphics): use before quoting. norm.",
        annotations(read_only_hint = true)
    )]
    pub async fn detect_foreign_content(
        &self,
        Parameters(p): Parameters<VersionParams>,
    ) -> Result<CallToolResult, ErrorData> {
        emit(domain::detect_foreign_content(
            &self.ctx,
            &p.eli_version,
            p.lang.as_deref(),
        ))
    }

    #[tool(
        name = "fedlex.find_treaties",
        description = "Find treaty processes (Staatsverträge) by a title word, partner country IRI or bilaterality: use to locate a treaty before get_treaty_info. hint.",
        annotations(read_only_hint = true)
    )]
    pub async fn find_treaties(
        &self,
        Parameters(p): Parameters<TreatySearchParams>,
    ) -> Result<CallToolResult, ErrorData> {
        emit(domain::find_treaties(
            &self.ctx,
            p.query.as_deref(),
            p.country.as_deref(),
            p.bilateral,
            p.limit,
        ))
    }

    #[tool(
        name = "fedlex.get_treaty_info",
        description = "Read a treaty process profile (title, signature, bilateral, partner countries, approving decree) for an eli/treaty IRI: use after find_treaties. norm.",
        annotations(read_only_hint = true)
    )]
    pub async fn get_treaty_info(
        &self,
        Parameters(p): Parameters<TreatyInfoParams>,
    ) -> Result<CallToolResult, ErrorData> {
        emit(domain::get_treaty_info(
            &self.ctx,
            &p.eli,
            p.lang.as_deref(),
        ))
    }

    #[tool(
        name = "fedlex.get_consultations",
        description = "List the consultations (Vernehmlassungen) of an act's drafts or of one draft, with status and dates: use for the genesis, never for law in force. hint.",
        annotations(read_only_hint = true)
    )]
    pub async fn get_consultations(
        &self,
        Parameters(p): Parameters<ConsultationsParams>,
    ) -> Result<CallToolResult, ErrorData> {
        emit(domain::get_consultations(
            &self.ctx,
            p.eli.as_deref(),
            p.draft.as_deref(),
            p.status.as_deref(),
            p.limit,
        ))
    }

    #[tool(
        name = "fedlex.get_consultation_documents",
        description = "List the position statements and result reports of one consultation IRI: use after get_consultations to read the genesis record. norm.",
        annotations(read_only_hint = true)
    )]
    pub async fn get_consultation_documents(
        &self,
        Parameters(p): Parameters<ConsultationDocsParams>,
    ) -> Result<CallToolResult, ErrorData> {
        emit(domain::get_consultation_documents(
            &self.ctx,
            &p.consultation,
        ))
    }

    #[tool(
        name = "fedlex.get_oc_act",
        description = "Resolve an act's binding AS/RO publication (oc ELI, date, genre, office, memorial) from its consolidation ELI: use to cite the Amtliche Sammlung. norm.",
        annotations(read_only_hint = true)
    )]
    pub async fn get_oc_act(
        &self,
        Parameters(p): Parameters<EliParams>,
    ) -> Result<CallToolResult, ErrorData> {
        emit(domain::get_oc_act(&self.ctx, &p.eli))
    }

    #[tool(
        name = "fedlex.get_memorial",
        description = "List the AS/BBl issue (memorial) an oc publication appeared in and the acts of that issue: use after get_oc_act to locate the volume. norm.",
        annotations(read_only_hint = true)
    )]
    pub async fn get_memorial(
        &self,
        Parameters(p): Parameters<MemorialParams>,
    ) -> Result<CallToolResult, ErrorData> {
        emit(domain::get_memorial(&self.ctx, &p.eli, p.limit))
    }

    #[tool(
        name = "fedlex.get_fga_documents",
        description = "List the Federal Gazette (BBl) documents of an act's genesis — Botschaft, reports — with genre and date: use for materials, never for law in force. norm.",
        annotations(read_only_hint = true)
    )]
    pub async fn get_fga_documents(
        &self,
        Parameters(p): Parameters<EliParams>,
    ) -> Result<CallToolResult, ErrorData> {
        emit(domain::get_fga_documents(&self.ctx, &p.eli))
    }

    #[tool(
        name = "fedlex.get_drafts",
        description = "List the legislative drafts (Entwürfe, eli/proj) an act came from, with the Curia Vista number: use as the entry to consultations and materials. norm.",
        annotations(read_only_hint = true)
    )]
    pub async fn get_drafts(
        &self,
        Parameters(p): Parameters<EliParams>,
    ) -> Result<CallToolResult, ErrorData> {
        emit(domain::get_drafts(&self.ctx, &p.eli))
    }
}

#[tool_handler(
    name = "oh-mcp-fedlex",
    version = "0.6.0",
    instructions = "OpenHelvetia fedlex domain server, base tier: the bitemporal \
         citation loop over the public Fedlex SPARQL endpoint, plus the \
         navigator surface (outline, in-act search, whole document, \
         references, amendment notes, annexes, tables, foreign content, \
         version comparison; article history, subdivisions, taxonomy, \
         language versions, vocabulary, related acts, formal citations, \
         node exploration) and the holdings beyond the SR (treaties, \
         consultations, drafts, the Official Compilation and the Federal \
         Gazette). parse_reference turns a quoted Fundstelle («Art. 7 Abs. 1 \
         lit. b LSV») into an act and an eId to read; the citation pair closes \
         the chain: check_quote proves a quote's wording against the element \
         that was read (never its truth), cite names that element's canonical \
         Fundstelle. Read-only, stateless; every content answer carries \
         provenance (kind norm/hint, valid_as_of, transaction_time); every \
         list is capped with a truncated flag and its original size. \
         search_law resolves official abbreviations (StPO, OR, ZGB) exactly \
         and ranks acts in force first; it is not a full-text search. \
         Manifestations are cached in-process: an answer from the cache says \
         served: cache and keeps the original retrieval moment as its \
         transaction_time. Discovery is two-stage: stage one is the one-line inventory \
         (tools/list here, meta.tools at the gateway) — each line names \
         when to use the tool and whether it answers a hint or a norm; \
         stage two loads the input schemas of the three to five tools you \
         intend to call (meta.schemas at the gateway). Live requests to the \
         federal endpoint pass a polite brake (2 a second, burst 4, at most \
         5 s of waiting): a call that would wait longer answers the typed \
         upstream-busy with retry_after_ms — wait that long, then retry; the \
         other refusals (not-found, invalid-input, upstream-unavailable) do \
         not recover by waiting. The loop: find the \
         act (resolve_sr or search_law) → pick the version (list_versions, \
         resolve_consolidation_at, list_expressions for PDF-only) → find \
         the place WITHOUT guessing article numbers (get_structure or \
         search_text) → read it (read_article) → cite only a norm. Policy \
         (auth, rate, budget) lives at the platform gateway, not here."
)]
impl ServerHandler for FedlexServer {}
