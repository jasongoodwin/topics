# topics-rs
A redis-pub/sub-inspired Rust demo/project showing an in memory topic store that publishes updates to consumers.

This project is a demonstration, not intended for production use.
See REDIS if you need something similar as the channels and pub/sub features are well developed.

## Status
This project was built to get me back into rust as it's been a few months.
It has some notes on rust usage hard won from a lot of days and nights building.
I've done some cool things with rust like build a distributed backend for Indradb. 
It has a steep initial learning curve but it's become my favorite language.

It's still a work in progress and needs some refactoring to allow testing. 
Specifically, the threads and "codec" section of the code can be factored and covered.
Testing and a client is the next step.

# Usage
The server listens on `0.0.0.0:8889` by default, or you can pass in a single argument when starting the application:
`cargo run 127.0.0.1:7777`

## How To Follow Along
You can test the server by using `telnet`.
(you may need to brew install telnet)
https://formulae.brew.sh/formula/telnet

Once you have telnet installed, you can connect to the server like so via terminal:

`telnet 127.0.0.1 8889`

Then you can issue space delimited commands:

`SUB mytopic`
`PUB mytopic I'm a message!`

You should see the responses.

Kick up a couple consoles and test this!
Benchmarks pending still but this project will absolutely shine against competitors, even given its youth.

## Protocol
This uses tokio's reactor for async.
It uses mpsc to manage connections to topics.

The protocol is simple:

A connection can publish a message to a topic like so:

`PUB topic I'm a message`

Connections are made and can subscribe to any topic by sending a message:
`SUB $topic`

Replies are provided to the socket - because the server has some asynchrony, it sends some information about the reply.
`OK SUB top1`

# Design
Each connection feeds messages to a single thread to maintain ordering.
There is a lock-free core task loop that will read requests and reply with updates to listener tasks.
This is done in a non-blocking fashion to require a small resource footprint while maintaining some asynchrony at the expense of needing the Tokio runtime in the project.
These trade-offs are well considered and I feel this is a good seed project for most any related use case.
It could be sharded and scaled to improve resource utilization depending on the use cases. It'll be very, very fast tho so only extreme applications would need to. 
consider moving in that direction.

At the core, there are three areas of interest:
- At the core, a single thread will process all activity to channels.
- The receiver loop will receive a connection and use mpsc for asynchronous communication

The thread will send requests over mpsc to topics where messages can be serially processed by each topic.
The topics will asynchronously return updates on any change to the main server thread, where the messages are dispatched async
back to any connections listening.
Each topic is guarded by an RW lock

## Outstanding Issues
There are a couple areas that I can see need some addressing:

### Connection Leaks!
There isn't any cleanup of the old connections. 
This project needs to signal to the core tread when someone is done.

# Observations and notes on Rust usage...
As the intention for this project was to get un-rusty, I was able to capture 
some of my experience here in some notes to aid my fellow rust users.
Despite some proximity from rust in the last months, I would describe my experience with rust as fairly advanced.
I've build a distributed database engine under Indra and done some pretty intense server-work with it.

While writing this I was able to collect some of my experience here and note some of my insights hard won
through a lot of really intense days and night. I have proficiency with many languages such as scala and elixir 
and I believe very strongly that rust is probably the most powerful language.
It has a steep initial learning-curve but it rewards teams with excellent safety.

## Memory usage and cloning
Through my learning, I've observed that people tend to not understand borrow vs ownership well and tend to over-clone.
You'll see careful handling of memory throughout the application.

## Cargo.lock
Cargo.lock is included in the project, but this should be removed.
Cargo.lock is best included with libraries and excluded in applications like this.
For simplicity, it's included but if this was a real stand-alone project intended for use,
this should be removed.

## Unwraps and Thread Panics
It's easy to unsafely `unwrap` and this is one of the areas that young teams will make mistakes.
Especially in multi-threaded environments, it's fairly easy to panic threads and not notice!
I've seen, for example, `riker` library has actors that will panic and not recover.
They die silently without any messaging and these bugs can easily be missed before hitting production.
https://github.com/riker-rs/riker

## Mixing Async/sync and spawning/blocking.
Another area I've seen errors made is in the boundaries between async and sync code.
While you can see in this app it's fully async, I've found bugs in applications that hand off between async boundaries.
See this PR of mine against indradb for example:
https://github.com/indradb/indradb/pull/235

It's pretty easy to deadlock tokio for example if not carefully handling the spawning of synchronous work.
These kinds of issues will fail silently and unexpectedly. Lessons learned!
People want to block on threads around these boundaries and it's pretty hard to understand this.
It's important when mixing asynchronous code with synchronous contexts that the hand-off and blocking is done utilizing spawn_blocking.
It's runtime-dependent but I've seen a lot of people run into this issue.
This is described here.
https://github.com/tokio-rs/tokio/issues/2376

Teams will often run into this when trying to use a sync server and internally using async code.
The servers need to use spawn_blocking for async to be utilized within the thread so consumers of sync servers will run into it a lot.

## Don't ignore warnings!
Use clippy and try to get compiler and `clippy` errors/warnings down to 0.
It's an important team heuristic to do early and often, linking this into CI if possible.
See: https://dev.to/cloudx/rust-and-the-hidden-cargo-clippy-2a2e

# Wait, Why No Tests? What about Metrics? A client?
I was getting un-rusty so I deferred writing tests - working on it.
Metrics here will incur some performance cost.
Feel free to fork or provide pr.
Rust is AWESOME.

# Linting?
Run `cargo clippy` to get some extra linting...

Yes the project has pretty much 0 warnings, yet there are a small number of clippy warnings left.
To verify, run `cargo clippy`. You'll see that there are a few warning on the enum capitalization (`PUB`, `SUB` etc) which can be fixed,
Not really feeling opinionated right now but should fix.

An important team-heuristic is to treat all warnings as errors and ensure they don't accumulate!
This is very very important to keep as a primary goal!
Lessons learned with rust.