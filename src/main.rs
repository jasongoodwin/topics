use std::borrow::Borrow;
use std::env;
use std::fmt::Debug;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio::sync::mpsc::{channel, Receiver, Sender};

use crate::protocol::{Frame, MessageType};

mod codec;
mod protocol;
mod pub_sub_topics;
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
    // drop is implemented only for example and to
    // demonstrate that the TopicSender is cleaned up appropriately.
    fn drop(&mut self) {
        println!("DEBUG - Client removed from memory! No leaks!: {}", self.id);
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

    let (tx, mut rx): (Sender<protocol::Frame>, Receiver<protocol::Frame>) = mpsc::channel(128);

    {
        // spawn a task to receive messages for the pub/sub engine.
        tokio::spawn(async move {
            let mut pub_sub_topics = pub_sub_topics::PubSubTopics::new();

            loop {
                if let Some(frame) = rx.recv().await {
                    pub_sub_topics.process_frame(frame).await;
                } else {
                    println!("DEBUG - [None] received by topics task");
                }
            }
        });
    }

    // Codec implementation. This could be modelled as the project grows (will make testing easier.)
    // Reads lines from the socket async into Frames which are then processed.
    // TODO[2022/Oct/05] shouldn't be any leaks, but may need some validation.
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
                        .write_all(&*frame.encode())
                        .await
                        .expect("failed to write frame back to socket");

                    if frame.0 == MessageType::QUIT {
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
                        0: MessageType::QUIT,
                        1: "".to_string(),
                        2: None,
                        3: topic_sender.clone(),
                    })
                    .await
                    .expect("something went really sideways");

                    return;
                }

                // note: we drop the newline bytes here. TODO[2022/Oct/05] move that to the codec.
                match Frame::decode(&buf[0..message_size - 2], topic_sender.clone()) {
                    Ok(frame) if frame.borrow().0.borrow() == &MessageType::QUIT => {
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
