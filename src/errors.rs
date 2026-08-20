#[derive(Debug, thiserror::Error)]
pub enum MParserError {
    #[error(transparent)]
    Chrono(#[from] chrono::ParseError),

    #[error("invalid statement period: {0}")]
    InvalidStatementPeriod(String),

    #[error("error parsing field: {0}")]
    StatementMeta(String),
}
