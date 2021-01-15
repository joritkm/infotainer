# WIP: infotainer

![Rust](https://github.com/joppich/infotainer/workflows/Rust/badge.svg)
[![codecov](https://codecov.io/gh/joppich/infotainer/branch/master/graph/badge.svg)](https://codecov.io/gh/joppich/infotainer)

Infotainer contains building blocks for simple pubsub services based on actix/-web, cbor and websockets.

## Components
* __[websocket interface](src/websocket.rs)__: handles client-server interaction
* __[pubsub service](src/pubsub.rs)__: manages subscriptions and handles publication of client submissions
* __[datalog service](src/data_log.rs)__: maintains log indices for and handles filesystem interactions
* __[session service](src/sessions.rs)__: manages client sessions

The websocket interface is an actor whose handlers are largely built around the `ClientCommand` and `ServerResponse` types. 
`ClientCommand` consists of struct variants holding data sent from client to server. These are translated into actix `Message`s by the `WebSocketSession` actor.  
The `ServerResponse` type is used to wrap data sent from server to client and can contain `HashSet<Uuid>`, representing a subscriptions log index, `Publication`, representing data published by some client, or `DataLogEntry`, containing one or more `Publication`s previously submitted to the data log.  

The pubsub service actor handles the `SubmitCommand` and `ManageSubscription` messages. When data is submitted for publication, the actor looks up the addressed `Subscription`, generates a `Publication`, sends it, wrapped in a `DataLogPut` message, to the `DataLogger` and proceeds to distribute it to connected clients subscribed to the `Subscription`.  
`ManageSubscription` has two variants, `Add` and `Remove`, and is used by clients to un-/subscribe from/to `Subscriptions`.

`DatalogService` enables persisting the server state as well as published data. `DataLogPut` messages represent write requests for `SubscriptionMeta`, subscriber lists and `Publication`s. `DataLogFetch` messages are used to retrieve persisted data. Additionally, it maintains a log of subscriptions and their publications.

The `SessionService` actor is a client registry, representing active sessions in a `HashMap<Uuid, Addr<WebSocketSession>>` and provides messages for clients to un-/register themselves as well as a message for other actors to resolve a clients ID to its WebSocketSession actors `Addr`.

## Features

- [x] websocket interface/ client message types
- [x] Subscription/Subscription table
- [x] Broadcast-type subscriptions
- [ ] Queue-type subscriptions
- [x] Publication
- [x] publishing messages
- [x] session management
- [x] data log service
- [x] persisting/retrieval of publication data
- [ ] persisting/retrieval of subscription metadata
- [ ] recreating server state from persisted data
- [ ] client service