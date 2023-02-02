use std::result::Result as StdResult;

/// Result is an type alias for a Result<T, Error> to reduce type noise.
/// This is a common pattern. It really does simplify the types in your project!
pub type Result<T> = StdResult<T, Box<dyn std::error::Error + Send + Sync>>;

/// InvalidMessage is an error.
#[derive(Debug, Clone)]
pub(crate) struct InvalidMessage {
    details: String,
}

impl InvalidMessage {
    // returns a boxed InvalidMessage in a result.
    pub(crate) fn new_result<T>(details: String) -> Result<T> {
        Err(Box::new(InvalidMessage { details }))
    }
}

impl std::fmt::Display for InvalidMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.details)
    }
}

impl std::error::Error for InvalidMessage {}

#[cfg(test)]
mod tests {
    use crate::result::InvalidMessage;

    #[test]
    // demonstrates we can capture the InvalidResult as an std::Result and print.
    // the abstraction just simplifies the types in code to make it more readable.
    fn invalid_message_should_print_debug_msg() {
        let msg: crate::result::Result<_> =
            InvalidMessage::new_result::<String>("these are the details".into());
        assert_eq!(
            format!("{:?}", msg),
            "Err(InvalidMessage { details: \"these are the details\" })"
        );
    }
}
