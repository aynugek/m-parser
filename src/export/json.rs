use m_parser::{MParserError, Statement};

// Exports a [`Statement`] as json
pub fn export(stmt: Statement) -> Result<Vec<u8>, MParserError> {
    Ok(serde_json::to_string(&stmt)?.into_bytes())
}
