//! Utils for config validations.

use std::path::Path;

use validator::{ValidationError, ValidationErrors, ValidationErrorsKind};

/// Custom validation for ASCII string.
pub fn validate_ascii(name: &impl ToString) -> Result<(), ValidationError> {
    if !name.to_string().is_ascii() {
        return Err(ValidationError::new("The value is not ASCII"));
    }
    Ok(())
}

/// Custom validation for file existence.
pub fn validate_file_exists(file_path: &Path) -> Result<(), ValidationError> {
    if !file_path.exists() {
        return Err(ValidationError::new("file not found"));
    }
    Ok(())
}

/// Custom validation for directory existence.
pub fn data_dir_exists(data_dir: &Path) -> Result<(), ValidationError> {
    if !data_dir.exists() {
        let mut error = ValidationError::new("directory not found");
        error.message = Some(
            "Please create a directory at the default ./data path or specify a different path \
             using the --storage.db_config.path_prefix flag."
                .into(),
        );
        return Err(error);
    }
    Ok(())
}

/// Struct for parsing a validation error.
pub struct ParsedValidationError {
    /// The path of the field that failed validation.
    pub path: String,
    /// The error code.
    pub code: String,
    /// The error message.
    pub message: Option<String>,
    /// The parameters of the error.
    pub params: String,
}

/// A vector of parsing validation errors.
pub struct ParsedValidationErrors(pub Vec<ParsedValidationError>);

impl TryFrom<ValidationErrors> for ParsedValidationErrors {
    type Error = String;

    fn try_from(errors: ValidationErrors) -> Result<Self, Self::Error> {
        let mut parsed_errors: ParsedValidationErrors = ParsedValidationErrors(vec![]);
        parse_validation_error(&errors, "".to_string(), &mut parsed_errors);
        Ok(parsed_errors)
    }
}

// This function gets an ValidationError object and parses it recursively to a ParsedValidationError
// object.
fn parse_validation_error(
    errors: &ValidationErrors,
    current_path: String,
    parsed_errors: &mut ParsedValidationErrors,
) {
    for (field, error) in errors.errors().iter() {
        let new_path = if current_path.is_empty() {
            field.to_string()
        } else {
            format!("{}.{}", current_path, field)
        };

        match error {
            ValidationErrorsKind::Struct(errors) => {
                parse_validation_error(errors, new_path, parsed_errors);
            }
            ValidationErrorsKind::List(errors) => {
                for (index, error) in errors.iter().enumerate() {
                    parse_validation_error(
                        error.1,
                        format!("{}[{}]", new_path, index),
                        parsed_errors,
                    );
                }
            }
            ValidationErrorsKind::Field(errors) => {
                for error in errors {
                    let parsed_error = ParsedValidationError {
                        path: new_path.to_owned(),
                        code: error.code.to_string(),
                        message: error.message.as_ref().map(|cow_string| cow_string.to_string()),
                        params: {
                            let params = &error.params;
                            params
                                .iter()
                                .map(|(_k, v)| v.to_string().replace('\"', ""))
                                .collect::<Vec<String>>()
                                .join(", ")
                        }
                        .to_owned(),
                    };
                    parsed_errors.0.push(parsed_error);
                }
            }
        }
    }
}
