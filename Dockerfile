# We split the Dockerfile into four stages
# The reason for that is to use Cargo Chef to compile dependecies before compiling the rest of the crates.
# This allows proper docker caching, and this stage will be cached until a dependecy change.
# When we change code in our crates this won't affect our cached layers.
# Before the stages we create a base image using 'clux/muslrust' with some additional required tools
# For more on docker stages, visit https://docs.docker.com/build/building/multi-stage/

#####################
# Stage 1 (planer): #
##################### 
# * Copy all src files
# * Running Cargo Chef prepare to generate recipe.json

#####################
# Stage 2 (cacher): #
##################### 
# * Copy recipe.json from planner stage
# * Compile all the dependecies using Cargo Chef cook

######################
# Stage 3 (builder): #
###################### 
# * Copy target dir (compiled deps) from cacher stage
# * Copy all repo src
# * Compile all crates and create a release

###########################
# Stage 4 (papyrus_node): #
###########################
# * Using Alpine Linux to run a lightweight and secured container
# * Copy our config directory from src
# * Copy the papyrus_node binary from our builder stage
# * Installing tini, for lightweight init system do be used to call our executable
# * Creating new user called papyrus with home directory /app
# * We expose ports 8080(RPC) and 8081(monitoring)
# * Finally we run papyrus_node with tini as an entrypoint


# Preparing the base image:
FROM clux/muslrust:1.78.0-stable AS chef
WORKDIR /app
RUN apt update && apt install -y clang unzip
RUN cargo install cargo-chef
ENV PROTOC_VERSION=25.1
RUN curl -L "https://github.com/protocolbuffers/protobuf/releases/download/v$PROTOC_VERSION/protoc-$PROTOC_VERSION-linux-x86_64.zip" -o protoc.zip && unzip ./protoc.zip -d $HOME/.local &&  rm ./protoc.zip
ENV PROTOC=/root/.local/bin/protoc

# Stage 1:
FROM chef AS planner
COPY . .
RUN cargo chef prepare

# Stage 2:
FROM chef AS cacher
COPY --from=planner /app/recipe.json recipe.json
# Build dependencies - this is the caching Docker layer!
RUN cargo chef cook --target x86_64-unknown-linux-musl --release --package papyrus_node

# Stage 3:
FROM chef AS builder
COPY . .
COPY --from=cacher /app/target target
ENV CARGO_INCREMENTAL=0
RUN rustup target add x86_64-unknown-linux-musl \
&& cargo build --target x86_64-unknown-linux-musl --release --package papyrus_node --locked

# Stage 4:
# Starting this stage so we have a clean lightweight and secured final image
FROM alpine:3.17.0 AS papyrus_node
ENV ID=1000
WORKDIR /app
# Copy the node executable and its config.
COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/papyrus_node /app/target/release/papyrus_node
COPY config config
# Installing tini
RUN set -ex; \
    apk update; \
    apk add --no-cache tini; \
    mkdir data
# Creating new user "papyrus"
RUN set -ex; \
    addgroup --gid ${ID} papyrus; \
    adduser --ingroup $(getent group ${ID} | cut -d: -f1) --uid ${ID} --gecos "" --disabled-password --home /app papyrus; \
    chown -R papyrus:papyrus /app
# Exposing rpc and monitoring ports
EXPOSE 8080 8081
USER ${ID}
# Finishing with our entrypoint
ENTRYPOINT ["/sbin/tini", "--", "/app/target/release/papyrus_node"]
