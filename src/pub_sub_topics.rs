use crate::{Frame, TopicSender};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Manages the topics and subscribers.
struct PubSubTopics {
    topics_state: HashMap<String, HashSet<Arc<TopicSender>>>,
}

impl PubSubTopics {
    pub(crate) fn new() -> PubSubTopics {
        PubSubTopics {
            topics_state: HashMap::default(),
        }
    }
}
