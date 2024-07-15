# Dockerfile with multi-stage builds for efficient dependency caching and lightweight final image.
# For more on Docker stages, visit: https://docs.docker.com/build/building/multi-stage/

# We use Cargo Chef to compile dependencies before compiling the rest of the crates.
# This approach ensures proper Docker caching, where dependency layers are cached until a dependency changes.
# Code changes in our crates won't affect these cached layers, making the build process more efficient.
# More info on Cargo Chef: https://github.com/LukeMathWalker/cargo-chef

# We start by creating a base image using 'clux/muslrust' with additional required tools.
FROM clux/muslrust:1.78.0-stable AS chef
WORKDIR /app
RUN apt update && apt install -y clang unzip
RUN cargo install cargo-chef
ENV PROTOC_VERSION=25.1
RUN curl -L "https://github.com/protocolbuffers/protobuf/releases/download/v$PROTOC_VERSION/protoc-$PROTOC_VERSION-linux-x86_64.zip" -o protoc.zip && unzip ./protoc.zip -d $HOME/.local &&  rm ./protoc.zip
ENV PROTOC=/root/.local/bin/protoc

#####################
# Stage 1 (planer): #
##################### 
FROM chef AS planner
COPY . .
# * Running Cargo Chef prepare that will generate recipe.json which will be used in the next stage.
RUN cargo chef prepare

#####################
# Stage 2 (cacher): #
##################### 
# Compile all the dependecies using Cargo Chef cook.
FROM chef AS cacher

# Copy recipe.json from planner stage
COPY --from=planner /app/recipe.json recipe.json

# Build dependencies - this is the caching Docker layer!
RUN cargo chef cook --target x86_64-unknown-linux-musl --release --package papyrus_node

######################
# Stage 3 (builder): #
###################### 
FROM chef AS builder
COPY . .
COPY --from=cacher /app/target target
# Disable incremental compilation for a cleaner build.
ENV CARGO_INCREMENTAL=0

# Add the target for x86_64-unknown-linux-musl and compile papyrus_node.
RUN rustup target add x86_64-unknown-linux-musl \
&& cargo build --target x86_64-unknown-linux-musl --release --package papyrus_node --locked

###########################
# Stage 4 (papyrus_node): #
###########################
# Uses Alpine Linux to run a lightweight and secure container.
FROM alpine:3.17.0 AS papyrus_node
ENV ID=1000
WORKDIR /app

# Copy the node executable and its configuration.
COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/papyrus_node /app/target/release/papyrus_node
COPY config config

# Install tini, a lightweight init system, to call our executable.
RUN set -ex; \
    apk update; \
    apk add --no-cache tini; \
    mkdir data

# Create a new user "papyrus".
RUN set -ex; \
    addgroup --gid ${ID} papyrus; \
    adduser --ingroup $(getent group ${ID} | cut -d: -f1) --uid ${ID} --gecos "" --disabled-password --home /app papyrus; \
    chown -R papyrus:papyrus /app

# Expose RPC and monitoring ports.
EXPOSE 8080 8081

# Switch to the new user.
USER ${ID}

# Set the entrypoint to use tini to manage the process.
ENTRYPOINT ["/sbin/tini", "--", "/app/target/release/papyrus_node"]
