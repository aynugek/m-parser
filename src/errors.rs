#[derive(Debug, thiserror::Error)]
pub enum MParserError {
    #[error(transparent)]
    Chrono(#[from] chrono::ParseError),

    #[error(transparent)]
    RustDecimal(#[from] rust_decimal::Error),

    #[error("invalid statement period: {0}")]
    InvalidStatementPeriod(String),

    #[error("error parsing field: {0}")]
    StatementMeta(String),

    #[error("invalid transaction type: {0}")]
    InvalidTransactionType(String),
}
