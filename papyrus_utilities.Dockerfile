# syntax = devthefuture/dockerfile-x

# The first line and the "INCLUDE Dockerfile" enable us to use the builder stage from the main Dockerfile.

# To build the papyrus utilities image, run from the root of the project:
# DOCKER_BUILDKIT=1 COMPOSE_DOCKER_CLI_BUILD=1 docker build -f papyrus_utilities.Dockerfile .

# TODO(dvir): consider adding the build of papyrus_utilities to the CI or to the nightly-test.

INCLUDE Dockerfile


# Build papyrus utilities.
FROM builder AS utilities_builder

# Build papyrus_load_test and copy its resources.
RUN CARGO_INCREMENTAL=0 cargo build --target x86_64-unknown-linux-musl --release --package papyrus_load_test --bin papyrus_load_test

# Build dump_declared_classes.
RUN CARGO_INCREMENTAL=0 cargo build --target x86_64-unknown-linux-musl --release --package papyrus_storage --bin dump_declared_classes

# Build train_compression_dictionary.
RUN CARGO_INCREMENTAL=0 cargo build --target x86_64-unknown-linux-musl --release --package papyrus_storage --bin train_compression_dictionary


# Starting a new stage so that the final image will contain only the executables.
FROM alpine:3.17.0 AS papyrus_utilities

# Copy the load test executable and its resources.
COPY --from=utilities_builder /app/target/x86_64-unknown-linux-musl/release/papyrus_load_test /app/target/release/papyrus_load_test
COPY crates/papyrus_load_test/resources/ /app/crates/papyrus_load_test/resources

# Copy the dump_declared_classes executable.
COPY --from=utilities_builder /app/target/x86_64-unknown-linux-musl/release/dump_declared_classes /app/target/release/dump_declared_classes

# Copy the train_compression_dictionary executable.
COPY --from=utilities_builder /app/target/x86_64-unknown-linux-musl/release/train_compression_dictionary /app/target/release/train_compression_dictionary

# Set the PATH environment variable to enable running an executable only with its name.
ENV PATH="/app/target/release:${PATH}"

ENTRYPOINT echo -e \
"There is no default executable for this image. Run an executable using its name or path to it.\n\
The available executables are:\n\
 - papyrus_load_test, performs a stress test on a node RPC gateway.\n\
 - dump_declared_classes, dumps the declared_classes table from the storage to a file.\n\
 - train_compression_dictionary, trains a compression dictionary for the storage.\n\
For example, in a docker runtime: docker run --entrypoint papyrus_load_test <image>"