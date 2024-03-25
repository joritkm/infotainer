# 2. Use grpc to build interface

Date: 2023-10-10

## Status
    🟢 Accepted

## Context

Service components need to expose well defined and documented APIs.
Clients and Service components need to be able to submit, read and 
stream data.

## Decision

We will use gRPC to implement service interfaces. It is well tested, 
widely available and covers all foreseeable use cases. 
Additionally, it enables highly performant inter process communication.

## Consequences

We will need to decide on a data format to create interface 
specifications and serialize commands and data passed through the 
interface. Clients and Service components will need to implement 
that implementation. 