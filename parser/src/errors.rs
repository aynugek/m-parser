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

impl MParserError {
    /// Returns `true` when the statement is encrypted and the PDF needs a
    /// password, including when the supplied password was rejected.
    pub fn is_password_error(&self) -> bool {
        matches!(self, Self::PdfInspector(pdf_inspector::PdfError::Encrypted))
    }
}

#[cfg(test)]
mod tests {
    use super::MParserError;

    #[test]
    fn encrypted_pdf_error_reports_a_password_error() {
        let error = MParserError::PdfInspector(pdf_inspector::PdfError::Encrypted);
        assert!(error.is_password_error());
    }

    #[test]
    fn other_errors_are_not_password_errors() {
        assert!(!MParserError::InvalidStatement.is_password_error());
        assert!(!MParserError::InsufficientData.is_password_error());
        assert!(
            !MParserError::PdfInspector(pdf_inspector::PdfError::InvalidStructure)
                .is_password_error()
        );
    }
}
