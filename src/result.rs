use std::result::Result as StdResult;

/// Result is an type alias for a Result<T, Error> to reduce type noise.
pub type Result<T> = StdResult<T, Box<dyn std::error::Error + Send + Sync>>;

// InvalidMessage is an error.
#[derive(Debug, Clone)]
pub(crate) struct InvalidMessage {
    details: String,
}

impl InvalidMessage {
    // returns a boxed InvalidMessage in a result.
    pub(crate) fn new<T>(details: String) -> Result<T> {
        Err(Box::new(InvalidMessage { details }))
    }
}

impl std::fmt::Display for InvalidMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.details)
    }
}

impl std::error::Error for InvalidMessage {}
