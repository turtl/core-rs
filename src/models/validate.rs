//! Defines a trait that performs model data validation.

use crate::error::{TResult, TError};

pub trait Validate {
    /// Determines if the model is fit for saving.
    ///
    /// Returns a vec of errors pairs (field, error) if there were problems.
    ///
    /// Override me!
    fn validate(&self) -> Vec<(String, String)> {
        Vec::new()
    }

    /// Called by the app, mainly, and used as a quick way to return an error
    /// if validaton fails.
    fn do_validate(&self, model_type: String) -> TResult<()> {
        let errors = self.validate();
        if errors.len() > 0 {
            return TErr!(TError::Validation(model_type, errors));
        }
        Ok(())
    }
}

/// Create an error entry
pub fn entry<T>(field: T, message: T) -> (String, String)
    where T: Into<String>
{
    (field.into(), message.into())
}

