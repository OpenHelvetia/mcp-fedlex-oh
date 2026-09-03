//! Fehlertyp der JOLux-Primitive.

use thiserror::Error;

/// Fehler bei einer JOLux-Primitive.
///
/// Die Primitive selbst geben `Result<_, JoluxError>` zurück. Die agentenseitige
/// Tool-Schicht übersetzt diese Fehler dann in lenkende `{ error, hint }`-Antworten
/// (Graceful Failure) — die Primitive bleiben ehrliche `Result`-Funktionen.
#[derive(Debug, Error, Clone)]
pub enum JoluxError {
    /// Transport-/Verbindungsfehler des SPARQL-Clients (transient — Retry hilft).
    #[error("SPARQL transport error: {0}")]
    Transport(String),

    /// Der SPARQL-Endpoint hat die Anfrage mit 4xx abgelehnt (Verify-L7):
    /// **permanent** — fast immer aus Nutzer-Eingaben gebaute Query
    /// (Sonderzeichen, Injection-Versuch), die den Endpoint zerbricht. Ein
    /// Retry ist zwecklos; die Tool-Schicht meldet das als Argument-Fehler,
    /// nicht als transienten Upstream-Ausfall (sonst Endlos-Retry-Falle).
    #[error("SPARQL rejected the query (HTTP {0})")]
    BadRequest(u16),

    /// Antwort war kein wohlgeformtes SPARQL-1.1-JSON.
    #[error("malformed SPARQL results: {0}")]
    MalformedResults(String),

    /// Erwartetes Ergebnis fehlte (leeres Binding-Set).
    #[error("no result for `{0}`")]
    NotFound(String),

    /// Ungültiger Identifier (ELI/ECLI).
    #[error(transparent)]
    Id(#[from] fedlex_core::IdError),
}
