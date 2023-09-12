//! Utils for config validations.

use std::path::Path;

use validator::ValidationError;

/// Custom validation for ASCII string.
pub fn validate_ascii(name: &impl ToString) -> Result<(), ValidationError> {
    if !name.to_string().is_ascii() {
        return Err(ValidationError::new("ASCII Validation"));
    }
    Ok(())
}

/// Custom validation for file existence.
pub fn validate_file_exists(path: &Path) -> Result<(), ValidationError> {
    if !path.exists() {
        return Err(ValidationError::new("File not found"));
    }
    Ok(())
}
