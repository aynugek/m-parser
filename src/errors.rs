#[derive(Debug, thiserror::Error)]
pub enum MParserError {
    #[error(transparent)]
    Chrono(#[from] chrono::ParseError),

    #[error(transparent)]
    RustDecimal(#[from] rust_decimal::Error),

    #[error(transparent)]
    PdfInspector(#[from] pdf_inspector::PdfError),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    SerdeJson(#[from] serde_json::Error),

    #[error("invalid statement period: {0}")]
    InvalidStatementPeriod(String),

    #[error("error parsing field: {0}")]
    StatementMeta(String),

    #[error("invalid transaction type: {0}")]
    InvalidTransactionType(String),

    #[error("invalid transaction status: {0}")]
    InvalidTransactionStatus(String),

    #[error("confirm your statement has both the summary and the detailed statement tables")]
    InsufficientData,

    #[error("invalid statement")]
    InvalidStatement,
}
