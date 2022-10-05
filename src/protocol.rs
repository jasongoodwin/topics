use std::sync::Arc;

use crate::protocol::MessageType::{PUB, SUB};
use crate::result::{InvalidMessage, Result};
use crate::TopicSender;

#[derive(Debug, Clone)]
pub(crate) enum MessageType {
    PUB,
    SUB,
    UPDATE,
    OK,
    // ERROR,
}

impl MessageType {
    fn new(typ: &str) -> Result<MessageType> {
        match typ.to_uppercase().as_str() {
            "PUB" => Ok(PUB),
            "SUB" => Ok(SUB),
            _ => InvalidMessage::new("Unknown message type".into()),
        }
    }
}

type Topic = String;
type Content = Option<String>;

/// A frame is a message received off the wire, or replied.
/// They contain a message type, the topic name, and optional content in string format.
#[derive(Debug, Clone)]
pub(crate) struct Frame(
    pub MessageType,
    pub Topic,
    pub Content,
    pub Arc<TopicSender>,
);

impl Frame {
    /// new parses bytes into a Result<Frame>
    /// Errors are generally of type InvalidMessage
    pub fn new(raw_msg: &[u8], sender: Arc<TopicSender>) -> Result<Frame> {
        let msg = std::str::from_utf8(raw_msg)?; // convert msg into str without the newline.

        match msg.split_once(' '){
            Some((typ, rest))=> {
                match MessageType::new(typ)?{
                    PUB => {
                        match rest.split_once(' ') {
                            None =>
                                InvalidMessage::new("invalid message format - needs to be in format `PUB $topic $message".to_string()),
                            Some((topic, message)) =>
                                Ok(Frame(
                                    PUB,
                                    topic.into(),
                                    Some(message.into()),
                                    sender
                                )),
                        }
                    }
                    SUB => {
                        // we get the topic and ignore anything after a whitespace.
                        let topic = rest.split(' ').nth(1).unwrap_or(rest);
                        Ok(Frame(
                            SUB,
                            topic.into(),
                            None,
                            sender
                        ))
                    }
                    _ =>
                        InvalidMessage::new( "invalid message format - needs to be in format `PUB $topic $message".to_string()),
                }
            }

            None =>
                InvalidMessage::new( "invalid message format - needs to be in format `PUB $topic` or `SUB $topic $optional_parameters".to_string()),
        }
    }
}
