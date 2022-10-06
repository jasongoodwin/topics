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
    QUIT,
    // TODO[2022/Oct/05] implement an ERROR
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

/// A frame is a message received off the wire, or replied. This is a codec.
/// They contain a message type, the topic name, and optional content in string format.
/// The frame is also the codec implementation.
/// It may be more efficient to use the tokio codec to stream bytes rather than passing arrays.
/// First pass, but this is pretty easy to understand.
#[derive(Debug, Clone)]
pub(crate) struct Frame(
    pub MessageType,
    pub Topic,
    pub Content,
    pub Arc<TopicSender>,
);

impl Frame {
    /// encode returns a frame as a set of bytes.
    pub fn encode(&self) -> Vec<u8> {
        // Just builds a String 'TYPE TOPIC CONTENT' and returns the referenced byte array.
        // bit more complex than needed to ensure no insignificant whitespace. Can rewrite this.

        // add the message_type
        let mut message: String = format!("{:?}", self.0);
        // add the topic
        if self.2.is_some() {
            message = message + &*format!(" {}", self.2.as_ref().unwrap())
        }
        // add the content
        if !self.1.is_empty() {
            message = message + &*format!(" {}", self.1)
        }
        message = message + "\n";
        message.as_bytes().to_owned()
    }

    /// decodes bytes into a Result<Frame>
    /// Errors are generally of type InvalidMessage
    /// TODO[2022/Oct/05] First iteration is getting outgrown - Would be a lot cleaner to stream through the bytes instead of splitting.
    pub fn decode(raw_msg: &[u8], sender: Arc<TopicSender>) -> Result<Frame> {
        let msg = std::str::from_utf8(raw_msg)?; // convert msg into str without the newline.

        match msg.split_once(' ') {
            _ if msg.to_uppercase() == "QUIT" => Ok(Frame(QUIT, "".to_string(), None, sender)),
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
                println!("DEBUG - bag msg: {}", msg);
                InvalidMessage::new( "invalid message format - needs to be in format `PUB $topic` or `SUB $topic $optional_parameters".to_string())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{Frame, MessageType, TopicSender};
    use std::sync::Arc;
    use tokio::sync::mpsc;
    use tokio::sync::mpsc::{Receiver, Sender};

    #[test]
    fn frame_should_decode_pub() {
        let (tx, mut rx): (Sender<Frame>, Receiver<Frame>) = mpsc::channel(128);
        let topic_sender = TopicSender::new(tx);

        let Frame(message_type, topic, content, sender) =
            Frame::decode("PUB topic message".as_ref(), topic_sender).unwrap();
        assert_eq!(message_type, MessageType::PUB);
        assert_eq!(topic, "topic");
        assert_eq!(content, Some("message".into()));
    }

    #[test]
    fn frame_should_decode_sub() {
        let (tx, mut rx): (Sender<Frame>, Receiver<Frame>) = mpsc::channel(128);
        let topic_sender = TopicSender::new(tx);

        let Frame(message_type, topic, content, sender) =
            Frame::decode("SUB topic".as_ref(), topic_sender).unwrap();
        assert_eq!(message_type, MessageType::SUB);
        assert_eq!(topic, "topic");
        assert_eq!(content, None);
    }

    #[test]
    fn frame_should_decode_quit() {
        let (tx, mut rx): (Sender<Frame>, Receiver<Frame>) = mpsc::channel(128);
        let topic_sender = TopicSender::new(tx);

        let Frame(message_type, topic, content, sender) =
            Frame::decode("QUIT".as_ref(), topic_sender).unwrap();
        assert_eq!(message_type, MessageType::QUIT);
        assert_eq!(topic, "");
        assert_eq!(content, None);
    }
}
