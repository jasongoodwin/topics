use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::{Frame, MessageType, TopicSender};

/// Manages the topics and subscribers.
/// This struct will hold the pub/sub TopicSender which contains a channel.
pub(crate) struct PubSubTopics {
    topics_state: HashMap<String, HashSet<Arc<TopicSender>>>,
}

impl PubSubTopics {
    pub(crate) fn new() -> PubSubTopics {
        PubSubTopics {
            topics_state: HashMap::default(),
        }
    }

    /// process_frame will handle a decoded message and reply on the appropriate channel.
    pub(crate) async fn process_frame(&mut self, frame: Frame) -> () {
        // no locking abstractions are needed as there is a single thread for the core engine.
        // This prevents any contention and will be faster than trying to manage locks.
        // Simple and to the point.
        // We can still parallelize sending of the messages tho to ensure it's extremely fast.

        let Frame(msg_type, topic, content, sender) = frame;
        match msg_type {
            MessageType::PUB => {
                sender
                    .clone()
                    .sender
                    .send(Frame(
                        MessageType::OK,
                        topic.clone(),
                        Some(format!("{:?}", msg_type)),
                        sender.clone(),
                    ))
                    .await
                    .expect("something went sideways..."); // should never happen

                match self.topics_state.get(&*topic) {
                    Some(subscribers) => {
                        let update_frame =
                            Frame(MessageType::UPDATE, topic, content, sender.clone());
                        for rcv in subscribers.iter() {
                            rcv.sender
                                .send(update_frame.clone())
                                .await
                                .expect("socket error");
                        }
                    }
                    None => {} // we don't care if there aren't subscribers as we don't maintain state.
                }
            }
            MessageType::SUB => {
                sender
                    .clone()
                    .sender
                    .send(Frame(
                        MessageType::OK,
                        topic.clone(),
                        Some(format!("{:?}", msg_type)),
                        sender.clone(),
                    ))
                    .await
                    .expect("something went sideways"); // This should never happen.

                if !self.topics_state.contains_key(&*topic.clone()) {
                    self.topics_state.insert(topic.clone(), HashSet::new());
                }
                let rcvs = self.topics_state.get_mut(&*topic.clone()).unwrap(); // safe unwrap!
                rcvs.insert(sender.clone());
            }
            MessageType::QUIT => {
                // Cleanup the connections on disconnect.
                // TODO[2022/Oct/05] - perf - Can be made linear to subscribed topics by keeping a reverse lookup
                // TODO[2022/Oct/05] leak - drop empty topics!
                sender
                    .clone()
                    .sender
                    .send(Frame(
                        MessageType::QUIT,
                        "".to_string(),
                        None,
                        sender.clone(),
                    ))
                    .await
                    .expect("something went sideways"); // This should never happen.

                println!(
                    "DEBUG - Client disconnected. cleaning up any subscriptions to prevent leak."
                );
                for (_topic, subscribers) in self.topics_state.iter_mut() {
                    subscribers.remove(&*sender.clone());
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
// Todo[2022/oct/05] test!
mod tests {}
