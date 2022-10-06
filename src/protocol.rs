use std::sync::Arc;

use crate::protocol::MessageType::{PUB, QUIT, SUB};
use crate::result::{InvalidMessage, Result};
use crate::TopicSender;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum MessageType {
    PUB,
    SUB,
    UPDATE,
    OK,
    QUIT, // ERROR,
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

        // TODO this can probably be improved: after adding "QUIT" message, you can see there may be no spaces after the command.
        match msg.split_once(' ') {
            Some((typ, rest)) => {
                match MessageType::new(typ)? {
                    PUB => match rest.split_once(' ') {
                        None => InvalidMessage::new(
                            "invalid message format - needs to be in format `PUB $topic $message"
                                .to_string(),
                        ),
                        Some((topic, message)) => {
                            Ok(Frame(PUB, topic.into(), Some(message.into()), sender))
                        }
                    },
                    SUB => {
                        // we get the topic and ignore anything after a whitespace.
                        let topic = rest.split(' ').nth(1).unwrap_or(rest);
                        Ok(Frame(SUB, topic.into(), None, sender))
                    }
                    _ => InvalidMessage::new(
                        "invalid message format - needs to be in format `PUB $topic $message"
                            .to_string(),
                    ),
                }
            }

            None => {
                if msg.to_uppercase() == "QUIT" {
                    Ok(Frame(QUIT, "".to_string(), None, sender))
                } else {
                    println!("msg: {}", msg);
                    InvalidMessage::new( "invalid message format - needs to be in format `PUB $topic` or `SUB $topic $optional_parameters".to_string())
                }
            }
        }
    }
}
