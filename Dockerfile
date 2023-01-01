FROM rust:1.63

RUN apt update && apt install -y clang vim

COPY . /app/

# Build the load test for CI, consider seperating to a different image.
WORKDIR /app/load_test
RUN cargo build --release -p papyrus_load_test

WORKDIR /app/

RUN cargo build --release --package papyrus_node --bin papyrus_node

RUN mkdir data

EXPOSE 8080 8081

ENTRYPOINT ["/app/target/release/papyrus_node"]
