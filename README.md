# topics-rs
Rust TCP Pub/Sub broker built from scratch. Inspired by Redis' protocol.

This project is a demonstration and for us to learn together! I haven't worked on it much or bench'd it but it should be feasible to get it screaming fast.

It's not intended for production use currently, although it would be a good project to build on.
See REDIS if you need something similar as the streams and pub/sub features are well developed. 

## Core Design and Guarantees
*Ordering Guaruntees* are strong across all topics (globally FIFO). By design, the core that holds the topics/subscribers is single-threaded, similar to Redis.
Messages produced by a single producer will always be delivered to consumers in order.
_Note: If topics were sharded, it should be able to scale-up and outperform redis at the expense of global message ordering._

*Delivery guarantee* is *AT MOST ONCE* delivery, and so this is better for streaming services instead of batch/event processing.
If you need at least once, you will do better with something like Redis Streams or Kafka that have ACKs for consumer/offset state, and message durability to ensure delivery through network failures. 

## Status
This project was built to get me back into rust as it's been a few months.
It has some notes on rust usage hard won from a lot of days and nights building.
I've done some cool things with rust like build a distributed backend for Indradb. 
Rust has a steep initial learning curve but it's become my favorite language.

Some notes are included in the readme for next steps.

# Usage
The server listens on `0.0.0.0:8889` by default, or you can pass in a single argument when starting the application:
`cargo run 127.0.0.1:7777`

# Testing
`cargo test` or `make test`

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

Kick up a couple of consoles and test this!
Benchmarks pending still but this project should be able to handle a lot of load.

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

If you're subscribed you'll receive an `UPDATE` message if anyone updates a topic with a message.
`UPDATE topic I'm a message`

# Design
Note:
I learned about the tokio codec abstractions recently: 
the tokio codec/Encoder/Decoder are not used at the moment.
An alternative design would use streams w/ the Tokio codec which I think is a good idea now.
See: https://docs.rs/tokio/0.1.22/tokio/codec/index.html

Each connection feeds messages to a single thread to ensure correct ordering.
There is a lock-free core task loop that will read requests and reply with updates to listeners.
This is done in a non-blocking fashion to require a small resource footprint while maintaining some asynchrony at the expense of needing the Tokio runtime in the project.
These trade-offs are well considered and I feel this is a good seed project for most any related use case.
It could be sharded and scaled to improve resource utilization depending on the use cases. It'll be very, very fast tho so only extreme applications would need to. 
consider moving in that direction.

At the core, there are some areas of interest:
- At the core, a single thread will process all activity to channels.
- The receiver loop will receive a connection and use mpsc for asynchronous communication.

The thread will send requests over mpsc to topics where messages can be serially processed by each topic.
The topics will asynchronously return updates on any change to the main server thread, where the messages are dispatched async
back to any connections listening.

## Outstanding Issues/Optimizations
There are a couple areas that I can see need some addressing:

### TEST
Tests are being added but still a bit anemic.
Good example project, but it needs some factoring and more tests still.

### Provide ERROR back to client
If invalid formats are provided, the server prints a message but doesn't reply to the client.

### Debug logging
It should have async and configurable debug logging to make it an easier demo project to look at and understand.
This is a great demo project!

### Improved Model
First pass, the project is nice and simple, but I learned that tokio provides stream abstractions.
The tokio `codec` provides a stream and this project would be a nice target to use those abstractions. 
The model is progressing and getting covered but I see some cool potential design targets.

### Spawn replies - stop awaiting/blocking.
There is an opportunity for further improved performance by not awaiting replies sent to the socket.
There are currently some serial awaits, but we can `join` those futures or `spawn` the work.

### Connection Leaks! [FIXED] 
Any subscriptions are cleaned up - can be made a bit more efficient - it's O(n) on number of topics to clean up.
Drop is implemented to print to demonstrate this works as expected. 

### Empty topic leak
When cleaning up subscriptions, topics should be emptied.

### Error replies
Currently, no error replies are sent to bad messages from clients.

# Observations and notes on Rust usage...
As the intention for this project was to get un-rusty, I was able to capture 
some of my experience here in some notes to aid my fellow rust users.
Despite some proximity from rust in the last months, I would describe my experience with rust as fairly advanced.
I've build a distributed database engine under Indra and done some pretty intense server-work with it.

While writing this I was able to collect some of my experience here and note some of my insights hard won
through a lot of really intense days and night. I have proficiency with many languages such as scala and elixir 
and I believe very strongly that rust is probably the most powerful language.
It has a steep initial learning-curve but it rewards teams with excellent safety.

## Memory usage and over-cloning
Through my learning, I've observed that people tend to not understand borrow vs ownership well and tend to over-clone.
I'll try to demonstrate this - use Rc and Arc and stop cloning everything!
I've seen advanced users of high calibre even not quite get this right.

Consider every call to `clone` - you can borrow and mutate and do all the things without making 1000 clones.
Primitives implement copy - they'll be replicated into function calls where they would be owned.
Utilizing Rc/Arc will allow cloning without a full memory copy.
You'll see channel halves cloned in this project - if you understand the implementations, then you'll understand that this is cheap!
https://docs.rs/tokio/latest/tokio/sync/mpsc/struct.Sender.html

You just need a little insight into the borrow/copy semantics, slices, primitives, and how the compiler works.
And be patient - it's really hard to learn these fine details. But just don't give up, backtrack on your learning, and do it as a team!
Hold sessions to gather and share insight and learning. It can be hard to balance progress and learning. 
Think: "As a team, let's do it together."

It's okay to over-use cloning if you're learning, and I promise you will.
But share, review, socialize. These are areas to progress on, not be perfect day 1.

## Cargo.lock
Cargo.lock is included in the project as it's the end of the dependency chain (eg a binary).

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

## "Speculative Generality" and Traits - Be careful with your OO paradigms
See Fowler's refactoring and the smell called "Speculative Generality" - the cost can be high in rust of trying to be general.

Due to the fact that trait size can't be known at compile time, utilizing traits can introduce a lot of boxing and harm readability.
You have to use dynamic dispatching, and it can just generally create a headache.

One of the mistakes I made early on in my rust usage was over-use of traits to avoid concrete implementations.
While this kind of approach is widely used in OO languages, in rust you may not need to do so (see the next point.)
It can seem to make testing easier, but there are some ways to get around this like having `#[cfg[test]]` blocks.
As an avid OO and FP person, it took me a while to find a good balance in rust.
Keep it simple until it's clearly worth paying the cost for the abstractions in rust.

## Stubs Without Traits?
One of the big reasons I was bullish in trying to abstract everything was to keep things in memory for test to have real unit tests.
There are some libraries that can swap implementations at run/test time, but I'd recommend you think about changing the 
imports w/ `#[cfg(test)]` and `#[cfg(not(test))]` annotations on the imports first.
This approach often really works and can avoid a lot of complexity. 

Eg:
```rust
#[cfg(test)]
use my_test_repo::MyTestRepo as MyRealRepo;
#[cfg(not(test))]
use my_real_repo::MyRealRepo;
```

Because they'll both have the same name, when the code is compiled in test, it'll just point to the other implementation.
No traits needed. No boxing. No dynamic dispatch.  

If you want mocks, there are a couple crates - namely `mockall` and `mockall-doubles` but they can be tricky to use 
in certain situations - notably mocking external dependencies can be hard and often overcome with design. 
Eg using a repository instead of directly access a database. This design approach is more "Domain Model" and less "Transaction Script."

See PoEAA from Fowler - Transaction Script vs Domain Model. 
https://martinfowler.com/eaaCatalog/transactionScript.html
https://martinfowler.com/eaaCatalog/domainModel.html
https://lorenzo-dee.blogspot.com/2014/06/quantifying-domain-model-vs-transaction-script.html

You can start with transaction scripts, but as complexity grows, teams will generally iterate toward domain models to better manage complexity.

## Don't fear the rewrite
"Slow, Imperfect Progress Is Better Than None at All."

One of the lessons for me in more recent years is that it's better to get something working today than it is to be paralyzed by a desire for perfection.

Iterate iterate iterate. You'll sleep and understand and see things without even trying.
Rust can be especially daunting to learn and work with for a beginner.

It's fine to start with something that works and reconsider the design as complexity grows.
Don't be worried about getting it perfect the first time.
This project is a living breathing example of this - I got something together, and it's "developing" daily.
Feel free to peruse the history to see the design progressing. I didn't even have tests in the first iteration!
Everything is a work in progress. Socialize these ideas, and make everyone feel comfortable and confident that "we'll get there!"
It's easy to get paralyzed looking for perfection.

## Standardizing a Makefile
Make is a c build tool but hold your judgement and hear me out:

We often work with a variety of technology in our teams and flipping between technologies has some cognitive load.
One of the ways I've found to sort of "standardize" the interaction with projects is to insert a Makefile in every project.

Rather than always needing to think about what `go` or `rust` or `python` or `elixir` or `javascript/html/css` targets are needed to test and build,
you can use a set of standard Makefile targets:

`make build`
`make test`
`make lint`
`make benchmark`
`make whatever!`

To do this, you just put a Makefile into the project root and call the appropriate tools and targets.
For example, instead of running `cargo fmt`, `cargo test` and `cargo build` you can call:

`make fmt` `make test` and `make build`

And do the same thing in each project.

Then every project your team uses, regardless of the technology, you can call the exact same targets.
It just standardizes the way you work with all of your technology so you don't have to go remember what build tools are used.
There are some beneficial side effects too. If a project lacks linting, testing, or benches, it'll be obvious.
It lets teams essentially standardize the workflow and reviewers have a net to catch more.
Make a document that says "here is how to review code: `make lint`, `make test`, `make bench` and also read/understand it!"

It almost seems silly simple but trust me - it's a powerful heuristic.
You can see the Makefile added to this project as an example, and just copy it into your other projects.
Doesn't matter what tech, you should always have at least some linting and formatting targets, even js/html/css.

## Some Notes on Runtimes (eg tokio), Async, and Blocking IO
Rust has some nice async abstractions with async/await, but core rust intentionally excludes any executor.
Threads are fine for some scenarios, but they have a fairly large overhead and can cause performance issues.
If there are many threads waiting for blocking IO (eg a database read,) then the CPU has to context swap all of the threads to find work.
This can cause poor CPU utilization - web applications are almost all async now and can perform orders of magnitude more work.

Because rust doesn't have any runtime in the core library, the ecosystem has been at work to build reactor-based runtimes.
Tokio is an example of a runtime and collection of tools that are well-developed and easy to implement.
The cost is the size of the dependencies and this is why there isn't anything in rust itself to address this problem.
Utilizing a runtime for task execution that's reactor-based will produce a small number of threads and use a scheduler to do the work in an efficient manner.
The caveat is that you need to be careful about blocking in an async scheduler as there are not many threads!
*THIS IS VERY IMPORTANT!*
See tokio's: `spawn_blocking` which will execute tasks that have blocking operations in a secondary dedicated threadpool and add threads as necessary.
https://teaclave.apache.org/api-docs/crates-app/tokio/task/fn.spawn_blocking.html#:~:text=Tokio%20will%20spawn%20more%20blocking,that%20cannot%20be%20performed%20asynchronously.


# Linting?
Run `cargo clippy` to get some extra linting...

Yes the project has pretty much 0 warnings, yet there are a small number of clippy warnings left.
To verify, run `cargo clippy`. You'll see that there are a few warning on the enum capitalization (`PUB`, `SUB` etc) which can be fixed,
Not really feeling opinionated right now but should fix.

An important team-heuristic is to treat all warnings as errors and ensure they don't accumulate!
This is very very important to keep as a primary goal!
Lessons learned with rust.
