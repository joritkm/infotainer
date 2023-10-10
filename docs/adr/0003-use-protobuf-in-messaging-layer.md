# 3. Use protobuf in messaging layer

Date: 2023-10-10

## Status

    🟢 Accepted

## Context

Commands and data need to be handled by the gRPC interface. This needs
to be specified and the resulting interface specification needs to be 
documented, so that clients and service components may implement 
interfaces.

## Decision

We will use Protobuf to create the interface specification. It is 
easy to use, widely available and the de-facto standard for gRPC 
interfaces. Additionally, it is performant and unifies specification 
and documentation.

## Consequences

Client and service components need to implement the specification. 
There's no need to write service or client specific specs. Downsides 
are that messages won't be humand readable and have to be serialized to 
protobufs binary format. Additionally, we won't get the zero cost 
abstraction and RPC authorization features something like cap'n'proto
would offer.
