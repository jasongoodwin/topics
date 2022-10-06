use std::collections::{HashMap, HashSet};
use std::env;
use std::fmt::Debug;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use crate::MessageType::QUIT;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio::sync::mpsc::{channel, Receiver, Sender};

use crate::protocol::{Frame, MessageType};

mod protocol;
mod result;

#[derive(Debug, Clone)]
/// This is stored w/ the sender half of a channel to manage the connections into the server.
/// Drop is just here for example so you can see that there aren't any leaks on disconnect.
struct TopicSender {
    id: String,
    sender: Sender<Frame>,
}

impl Hash for TopicSender {
    // hash is implemented to allow storing in the hashmap.
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl PartialEq for TopicSender {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Drop for TopicSender {
    fn drop(&mut self) {
        println!("DEBUG - Client removed from memory! No leaks!: {}", self.id);
        // self.sender.send(Frame{
        //     0: MessageType::QUIT,
        //     1: "".to_string(),
        //     2: None,
        //     3: Arc::new(*self)
        // })
    }
}

impl Eq for TopicSender {}

/// You can pass in the listening host/port as a parameter
/// or else it will use port 8889.
#[tokio::main]
async fn main() -> crate::result::Result<()> {
    let addr: String = env::args()
        .nth(1)
        .unwrap_or_else(|| "0.0.0.0:8889".to_string());

    let listener = TcpListener::bind(&addr).await?;
    // FIXME - has a leak - needs to clean up disconnected clients!
    // Can use Drop to signal to the core.

    let (tx, mut rx): (Sender<protocol::Frame>, Receiver<protocol::Frame>) = mpsc::channel(128);

    {
        // spawn a task to receive messages for the pub/sub engine.
        tokio::spawn(async move {
            // no locking abstractions are needed as there is a single thread for the core engine.
            // This prevents any contention and will be faster than trying to manage locks.
            // Simple and to the point.
            // We can still parallelize sending of the messages tho to ensure it's extremely fast.
            let mut state: HashMap<String, HashSet<Arc<TopicSender>>> = HashMap::default();

            loop {
                if let Some(Frame(msg_type, topic, content, sender)) = rx.recv().await {
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

                            match state.get(&*topic) {
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

                            if !state.contains_key(&*topic.clone()) {
                                state.insert(topic.clone(), HashSet::new());
                            }
                            let rcvs = state.get_mut(&*topic.clone()).unwrap(); // safe unwrap!
                            rcvs.insert(sender.clone());
                        }
                        MessageType::QUIT => {
                            // Cleanup the connections on disconnect.
                            // TODO this is O(n) where n is topics.
                            // Can be made linear to subscribed topics by keeping a reverse lookup
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

                            println!("DEBUG - Client disconnected. cleaning up any subscriptions to prevent leak.");
                            for (_topic, subscribers) in state.iter_mut() {
                                subscribers.remove(&*sender.clone());
                            }
                        }
                        _ => {}
                    }
                } else {
                    println!("DEBUG - [None] received by topics task");
                }
            }
        });
    }

    // Codec implementation. This could be modelled as the project grows (will make testing easier.)
    // Reads lines from the socket async into Frames which are then processed.
    // TODO shouldn't be any leaks, but may need some validation.
    loop {
        let (socket, _): (TcpStream, _) = listener.accept().await?;
        let (mut socket_read, mut socket_write): (OwnedReadHalf, OwnedWriteHalf) =
            socket.into_split();
        let tx = tx.clone();

        let (reply_tx, mut reply_rx): (Sender<Frame>, Receiver<Frame>) = channel(128);

        let topic_sender = Arc::new(TopicSender {
            id: uuid::Uuid::new_v4().to_string(),
            sender: reply_tx,
        });

        // In one task loop, we await replies and send to the write side of the socket.
        // Note this needs to receive a QUIT
        tokio::spawn(async move {
            loop {
                if let Some(frame) = reply_rx.recv().await {
                    socket_write
                        .write_all(
                            format!(
                                "{:?} {} {}\n",
                                frame.0,
                                frame.2.unwrap_or_else(|| "".into()),
                                frame.1
                            )
                            .as_ref(), // TODO refactor to implement in Display.
                        )
                        .await
                        .expect("failed to write frame back to socket");

                    if frame.0 == QUIT {
                        break;
                    }
                }
            }
            println!("DEBUG - terminating client connection.");
        });

        tokio::spawn(async move {
            // create a 1024 byte buffer and use this to receive lines.
            // telnet will only allow 512 characters for example, so this is okay for a demo.
            // This simplifies the code greatly. A more in-depth approach is demonstrated here:
            // https://v0-1--tokio.netlify.app/docs/going-deeper/chat/
            let mut buf = vec![0; 1024];

            // In another sync loop, we read data from the socket and send to the codec core
            loop {
                // we read a line async into the buffer. n captures the length of the line.
                let message_size = socket_read
                    .read(&mut buf)
                    .await
                    .expect("failed to read data from socket (disconnect)");

                if message_size < 2 {
                    println!("disconnect received");
                    // Likely a FIN was ACK'd. On OSX, the socket advertises a single byte read (0x4).
                    tx.send(Frame {
                        // TODO move to a method on `Frame`
                        0: MessageType::QUIT,
                        1: "".to_string(),
                        2: None,
                        3: topic_sender.clone(),
                    })
                    .await
                    .expect("something went really sideways");

                    return;
                }

                // note: we drop the newline bytes here.
                match Frame::new(&buf[0..message_size - 2], topic_sender.clone()) {
                    Ok(frame) if frame.0 == QUIT => {
                        println!("quit");
                        tx.send(frame)
                            .await
                            .expect("something went really sideways");
                        break; // terminate task.
                    }
                    Ok(frame) => tx
                        .send(frame)
                        .await
                        .expect("something went really sideways"),
                    Err(e) => println!("error: [{:?}]", e),
                };
            }
            println!("DEBUG - task terminated.");
        });
    }
}
