//! Utils for config validations.

use validator::{Validate, ValidationError};

/// Custom validation for recursive config validation.
pub fn recursive_validation(inner_config: &impl Validate) -> Result<(), ValidationError> {
    if inner_config.validate().is_err() {
        return Err(ValidationError::new("Config Validation"));
    }
    Ok(())
}

/// Custom validation for ASCII string.
pub fn validate_ascii(name: &impl ToString) -> Result<(), ValidationError> {
    if !name.to_string().is_ascii() {
        return Err(ValidationError::new("ASCII Validation"));
    }
    Ok(())
}
