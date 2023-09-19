//! Utils for config validations.

use std::fmt::Display;
use std::path::Path;

use validator::{ValidationError, ValidationErrors, ValidationErrorsKind};

/// Custom validation for ASCII string.
pub fn validate_ascii(name: &impl ToString) -> Result<(), ValidationError> {
    if !name.to_string().is_ascii() {
        return Err(ValidationError::new("The value is not ASCII"));
    }
    Ok(())
}

/// Custom validation for file or directory path existence.
pub fn validate_path_exists(file_path: &Path) -> Result<(), ValidationError> {
    if !file_path.exists() {
        let mut error = ValidationError::new("file or directory not found");
        error.message = Some(
            "Please create the file/directory or change the path in the configuration.".into(),
        );
        return Err(error);
    }
    Ok(())
}

/// Struct for parsing a validation error.
pub struct ParsedValidationError {
    /// The path of the field that failed validation.
    pub param_path: String,
    /// The error code.
    pub code: String,
    /// The error message.
    pub message: Option<String>,
    /// The parameters of the error.
    pub params: String,
}

/// A vector of parsing validation errors.
pub struct ParsedValidationErrors(pub Vec<ParsedValidationError>);

impl From<ValidationErrors> for ParsedValidationErrors {
    fn from(errors: ValidationErrors) -> Self {
        let mut parsed_errors: ParsedValidationErrors = ParsedValidationErrors(vec![]);
        parse_validation_error(&errors, "".to_string(), &mut parsed_errors);
        parsed_errors
    }
}

impl Display for ParsedValidationErrors {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut error_string = String::new();
        for error in &self.0 {
            error_string.push_str(&format!(
                "Configuration error: The field {:?} has an error {:?} with value: {:?} \n",
                &error.param_path, &error.code, &error.params
            ));
            if let Some(msg) = &error.message {
                error_string.push_str(&format!("{:?} \n", msg));
            }
        }
        write!(f, "{}", error_string)
    }
}

// This function gets a ValidationError object and parses it recursively to a ParsedValidationError
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
                        param_path: new_path.to_owned(),
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
