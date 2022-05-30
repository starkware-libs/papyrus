# 2. Implementation in Single Process

Date: 2022-05-25

## Status

Accepted

## Context

When implementing a full network node, we identify several high level components
deal with various aspects of the node functionality, ranging from lower level functionality (e.g.
network communication) to higher level domain, e.g. the synchronization process and API endpoint.

When considering how these components interact, we consider whether these should be deployed in
separate processes or as a single deployable unit communicating in memory.

We identify 2 main reasons to separate a component into a separate process:
1. A component needs to scale differently then other components it interacts with. In other words
two interacting components process the relevant data in different speeds.
2. A component needs to be implemented in a different technology than the rest of the node (e.g. a
python component in a rust-based binary).

At the same time, we balance this with the motivation to keep the node simple to develop, deploy and
operate. Also, developing everything as one process makes development (build, refactoring, etc)
easier.

## Decision

We decide that the node will be implemented as a single process.
All components interact directly through function calls.

## Consequences

We will consider the reasons stated above when we incorporate new functionality.
One such example might be embedding a Cairo VM.
