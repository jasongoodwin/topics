mod codec;
mod protocol;
mod result;

use crate::protocol::{Frame, MessageType};
use dashmap::{DashMap, DashSet};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::env;
use std::fmt::Debug;
use std::hash::{Hash, Hasher};
use std::net::SocketAddr;
use std::rc::Rc;
use std::str::Utf8Error;
use std::sync::{Arc, RwLock};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf, ReadHalf, WriteHalf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::sync::{mpsc, Mutex};

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

#[tokio::main]
async fn main() -> crate::result::Result<()> {
    let addr: String = env::args()
        .nth(1)
        .unwrap_or_else(|| "0.0.0.0:8889".to_string());

    let listener = TcpListener::bind(&addr).await?;

    // topics and list of mpsc senders backed by sockets.
    // TODO need to clean up disconnected clients!
    // let state: Arc<DashMap<String, DashSet<TopicSender>>> = Arc::new(DashMap::default());
    // let state: Arc<RwLock<HashMap<String, Arc<RwLock<HashSet<TopicSender>>>>>> = Arc::new(RwLock::new(HashMap::default()));

    let (tx, mut rx): (Sender<protocol::Frame>, Receiver<protocol::Frame>) = mpsc::channel(128);

    {
        // spawn a task to receive messages for the pub/sub engine.
        tokio::spawn(async move {
            // no locking abstractions are needed as there is a single thread for the core engine.
            // This prevents any contention and will be significantly faster than trying to manage across cores.
            // We can still parallelize sending of the messages tho to ensure it's extremely fast.
            let mut state: HashMap<String, HashSet<Arc<TopicSender>>> = HashMap::default();

            loop {
                if let Some(frame) = rx.recv().await {
                    println!("got a msg on core");
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
                                    .await;
                                // let lock = state.read().unwrap(); // fixme unsafe unwrap
                                match state.get(&*topic) {
                                    None => {
                                        println!("publishing to topic with no subscribers");
                                        // we don't currently care if there aren't subscribers as we don't maintain state.
                                    }
                                    Some(subscribers) => {
                                        println!("publishing to topic with subscribers");
                                        let update_frame = Frame(
                                            MessageType::UPDATE,
                                            topic,
                                            content,
                                            sender.clone(),
                                        );
                                        for rcv in subscribers.iter() {
                                            println!("sending {:?} {:?}", rcv.id, msg_type);
                                            rcv.sender.send(update_frame.clone()).await;
                                            // todo - put the frame in an arc to prevent necessity of clones.
                                        }
                                    }
                                }

                                // if state.contains_key(&*topic) {
                                //     for rcv in rcvs.iter() {
                                //         rcv.send(frame.clone()); // todo - put the frame in an arc to prevent necessity of clones.
                                //     }
                                // } else {}

                                // let rcvs: DashSet<_> = state.get_or_insert(topic, DashSet::new());
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
                                    .await;
                                println!("subscribing");

                                // let mut lock = state.write().unwrap(); // fixme unsafe unwrap
                                if !state.contains_key(&*topic.clone()) {
                                    state.insert(topic.clone(), HashSet::new());
                                }
                                // let lock = state.write().unwrap(); // fixme unsafe unwrap
                                let mut rcvs = state.get_mut(&*topic.clone()).unwrap(); // safe unwrap!
                                rcvs.insert(sender.clone());
                            }
                            // MessageType::UPDATE => {}
                            // MessageType::OK => {}
                            // MessageType::ERROR => {}
                            _ => {}
                        }
                    }
                } else {
                    println!("[None] received by topics task");
                }
            }
        });
    }

    // Codec implementation. Should be modelled as such.
    // Reads lines from the socket async into Frames which are then processed.
    loop {
        let (mut socket, _): (TcpStream, _) = listener.accept().await?;
        let (mut socket_read, mut socket_write): (OwnedReadHalf, OwnedWriteHalf) =
            socket.into_split();
        // let state = state.clone();
        let tx = tx.clone();

        let (reply_tx, mut reply_rx): (Sender<Frame>, Receiver<Frame>) = mpsc::channel(128);

        let topic_sender = Arc::new(TopicSender {
            id: uuid::Uuid::new_v4().to_string(),
            sender: reply_tx,
        });

        // In one task loop, we await replies and send to the write side of the socket.
        tokio::spawn(async move {
            loop {
                if let Some(frame) = reply_rx.recv().await {
                    println!("here");
                    println!("trying to send message: :{:?}", frame.0);
                    // replies w/ a response to the request. Some examples:
                    // "OK SUB $topic"
                    // "ERROR SUB $topic $error_message"
                    // "UPDATE $topic $new_state"
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

                println!("got {:?} characters", message_size);

                if message_size < 2 {
                    // Likely a FIN was ACK'd. On OSX, the socket advertises a single byte read (4).
                    println!("closing connection");
                    return;
                }

                // note: we drop the newline character here.
                match Frame::new(&buf[0..message_size - 2], topic_sender.clone()) {
                    // todo sender in arc.
                    Ok(frame) => {
                        println!("got a frame {:?}", frame.0.clone());
                        tx.send(frame).await;
                        // if let Frame(msg_type, topic, content, sender) = frame {
                        //     match msg_type {
                        //         MessageType::PUB => {
                        //             let lock = state.read().unwrap(); // fixme unsafe unwrap
                        //             match lock.get(&*topic) {
                        //                 None => {
                        //                     println!("publishing to topic with no subscribers");
                        //                     // we don't currently care if there aren't subscribers as we don't maintain state.
                        //                 }
                        //                 Some(_) => {
                        //
                        //                 }
                        //             }
                        //
                        //             if lock.contains_key(&*topic) {
                        //                 for rcv in rcvs.iter() {
                        //                     rcv.send(frame.clone()); // todo - put the frame in an arc to prevent necessity of clones.
                        //                 }
                        //             } else {
                        //
                        //             }
                        //
                        //
                        //
                        //             // let rcvs: DashSet<_> = state.get_or_insert(topic, DashSet::new());
                        //
                        //
                        //         }
                        //         MessageType::SUB => {
                        //             let mut lock = state.write().unwrap(); // fixme unsafe unwrap
                        //             if !lock.contains_key(&*topic) {
                        //                 lock.insert(topic, Arc::new(RwLock::new(HashSet::new())));
                        //             }
                        //             // let lock = state.write().unwrap(); // fixme unsafe unwrap
                        //             // let rcvs: DashSet<_> = state.get_or_insert(topic, DashSet::new());
                        //             // rcvs.insert(topic_sender.clone());
                        //         }
                        //         // MessageType::UPDATE => {}
                        //         // MessageType::OK => {}
                        //         // MessageType::ERROR => {}
                        //         _ => {}
                        //     }
                        //
                        //     sender.sender.send(Frame(MessageType::OK, topic.clone(), Some(format!("{:?}", msg_type)), topic_sender.clone())).await;
                        // }
                    }
                    Err(e) => println!("ignoring line due to error: {:?}", e),
                };
            }
        });
    }
}

fn process_message(msg: &str) -> () {}

/// Shorthand for the transmit half of the message channel.
type Tx = mpsc::UnboundedSender<String>;

/// Shorthand for the receive half of the message channel.
type Rx = mpsc::UnboundedReceiver<String>;
//
// async fn process(socket: TcpStream) -> crate::result::Result<String> {
//     println!("here!");
//     // The `Connection` lets us read/write redis **frames** instead of
//     // byte streams. The `Connection` type is defined by mini-redis.
//     // let mut connection = Connection::new(socket);
//     //
//     // if let Some(frame) = connection.read_frame().await.unwrap() {
//     //     println!("GOT: {:?}", frame);
//     //
//     //     Respond with an error
//         // let response = Frame::Error("unimplemented".to_string());
//         // connection.write_frame(&response).await.unwrap();
//     // }
//     Ok("Ok".into())
// }

/// Process an individual chat client
async fn process(
    state: Arc<DashMap<String, String>>,
    stream: TcpStream,
    addr: SocketAddr,
) -> crate::result::Result<String> {
    //     let mut lines = Framed::new(stream, LinesCodec::new());
    //
    //     // Send a prompt to the client to enter their username.
    //     lines.send("Please enter your username:").await?;
    //
    //     // Read the first line from the `LineCodec` stream to get the username.
    //     let username = match lines.next().await {
    //         Some(Ok(line)) => line,
    //         // We didn't get a line so we return early here.
    //         _ => {
    //             tracing::error!("Failed to get username from {}. Client disconnected.", addr);
    //             return Ok(());
    //         }
    //     };
    //
    //     // Register our peer with state which internally sets up some channels.
    //     let mut peer = Peer::new(state.clone(), lines).await?;
    //
    //     // A client has connected, let's let everyone know.
    //     {
    //         let mut state = state.lock().await;
    //         let msg = format!("{} has joined the chat", username);
    //         tracing::info!("{}", msg);
    //         state.broadcast(addr, &msg).await;
    //     }
    //
    //     // Process incoming messages until our stream is exhausted by a disconnect.
    //     loop {
    //         tokio::select! {
    //             // A message was received from a peer. Send it to the current user.
    //             Some(msg) = peer.rx.recv() => {
    //                 peer.lines.send(&msg).await?;
    //             }
    //             result = peer.lines.next() => match result {
    //                 // A message was received from the current user, we should
    //                 // broadcast this message to the other users.
    //                 Some(Ok(msg)) => {
    //                     let mut state = state.lock().await;
    //                     let msg = format!("{}: {}", username, msg);
    //
    //                     state.broadcast(addr, &msg).await;
    //                 }
    //                 // An error occurred.
    //                 Some(Err(e)) => {
    //                     tracing::error!(
    //                         "an error occurred while processing messages for {}; error = {:?}",
    //                         username,
    //                         e
    //                     );
    //                 }
    //                 // The stream has been exhausted.
    //                 None => break,
    //             },
    //         }
    //     }
    //
    //     // If this section is reached it means that the client was disconnected!
    //     // Let's let everyone still connected know about it.
    //     {
    //         let mut state = state.lock().await;
    //         state.peers.remove(&addr);
    //
    //         let msg = format!("{} has left the chat", username);
    //         tracing::info!("{}", msg);
    //         state.broadcast(addr, &msg).await;
    //     }
    println!("here!");
    Ok("Ok".into())
}
