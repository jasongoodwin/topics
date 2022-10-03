use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::env;
use std::fmt::Debug;
use std::hash::{Hash, Hasher};
use std::net::SocketAddr;
use std::rc::Rc;
use std::str::Utf8Error;
use std::sync::{Arc, RwLock};

use dashmap::{DashMap, DashSet};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf, ReadHalf, WriteHalf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::sync::{mpsc, Mutex};

use crate::protocol::{Frame, MessageType};

mod protocol;
mod result;

#[derive(Debug, Clone)]
struct TopicSender {
    id: String,
    sender: Sender<Frame>,
}

impl Hash for TopicSender {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl PartialEq for TopicSender {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
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
                if let Some(frame) = rx.recv().await {
                    if let Frame(msg_type, topic, content, sender) = frame.clone() {
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
                                        let update_frame = Frame(
                                            MessageType::UPDATE,
                                            topic,
                                            content,
                                            sender.clone(),
                                        );
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
                                let mut rcvs = state.get_mut(&*topic.clone()).unwrap(); // safe unwrap!
                                rcvs.insert(sender.clone());
                            }
                            _ => {}
                        }
                    }
                } else {
                    println!("[None] received by topics task");
                }
            }
        });
    }

    // Codec implementation. This could be modelled as the project grows.
    // Reads lines from the socket async into Frames which are then processed.
    loop {
        let (mut socket, _): (TcpStream, _) = listener.accept().await?;
        let (mut socket_read, mut socket_write): (OwnedReadHalf, OwnedWriteHalf) =
            socket.into_split();
        let tx = tx.clone();

        let (reply_tx, mut reply_rx): (Sender<Frame>, Receiver<Frame>) = channel(128);

        let topic_sender = Arc::new(TopicSender {
            id: uuid::Uuid::new_v4().to_string(),
            sender: reply_tx,
        });

        // In one task loop, we await replies and send to the write side of the socket.
        tokio::spawn(async move {
            loop {
                if let Some(frame) = reply_rx.recv().await {
                    socket_write
                        .write_all(
                            format!(
                                "{:?} {} {}\n",
                                frame.0,
                                frame.2.unwrap_or("".into()),
                                frame.1
                            )
                            .as_ref(), // TODO refactor to implement in Display.
                        )
                        .await
                        .expect("failed to write frame back to socket");
                }
            }
        }); // will crash if this doesn't return ok.

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
                    // Likely a FIN was ACK'd. On OSX, the socket advertises a single byte read (0x4).
                    return;
                }

                // note: we drop the newline bytes here.
                match Frame::new(&buf[0..message_size - 2], topic_sender.clone()) {
                    Ok(frame) => tx
                        .send(frame)
                        .await
                        .expect("something went really sideways"),
                    Err(e) => println!("error: [{:?}]", e),
                };
            }
        });
    }
}
