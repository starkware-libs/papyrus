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
pub fn validate_file_exists(file_path: &Path) -> Result<(), ValidationError> {
    if !file_path.exists() {
        let mut error = ValidationError::new("File not found");
        error.message = Some(format!("File '{}' does not exist.", file_path.display()).into());
        return Err(error);
    }
    Ok(())
}
