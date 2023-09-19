//! Utils for config validations.

use std::fmt::Display;
use std::path::Path;

use validator::{Validate, ValidationError, ValidationErrors, ValidationErrorsKind};

use crate::ConfigError;

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
#[derive(Debug)]
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
#[derive(thiserror::Error, Debug)]
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
                "Configuration error: The field: {} has an error: {} with value: {}, {} \n",
                &error.param_path,
                &error.code,
                &error.params,
                &error.message.clone().unwrap_or("".to_string()),
            ));
        }
        error_string = error_string.replace('\"', "");
        write!(f, "{}", error_string)
    }
}

/// A wrapper function for the validator validate function that returns a ConfigError with ParsedValidationErrors.
pub fn config_validate<T: Validate>(config: &T) -> Result<(), ConfigError> {
    config.validate().map_err(|errors| {
        ConfigError::ConfigValidationError(ParsedValidationErrors::from(errors))
    })
}

// This function gets a ValidationError object and parses it recursively to a ParsedValidationError
// object to make it readable for the user.

// Example of a ValidationError object printed:
// ValidationErrors({"storage": Struct(ValidationErrors({"db_config":
// Struct(ValidationErrors({"path_prefix": Field([ValidationError { code: "file or directory not
// found", message: Some("Please create the file/directory or change the path in the
// configuration."), params: {"value": String("./data")} }])}))}))})

// Example of a ParsedValidationError object printed:
// Configuration error: The field "storage.db_config.path_prefix" has an error "file or directory
// not found" with value: "./data" "Please create the file/directory or change the path in the
// configuration."

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
