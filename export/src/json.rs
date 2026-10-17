use parser::Statement;

use crate::ExportError;

// Exports a [`Statement`] as json
pub fn export(stmt: Statement) -> Result<Vec<u8>, ExportError> {
    Ok(serde_json::to_string(&stmt)?.into_bytes())
}
