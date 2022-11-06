FROM rust:1.63

# We will at first compile only the dependency crates and then we'll compile the source code. This
# will cause the compilation of the dependency crates to be cached even when the source code is
# changed.

# Copy all files and then delete non-Cargo.toml files. Because the compilation will happen in a
# different stage, the non-Cargo.toml files won't affect the cache (For more on docker stages, read
# https://docs.docker.com/build/building/multi-stage/).
COPY . /app/
RUN find /app \! -name "Cargo.toml" -type f -delete 
RUN find /app -empty -type d -delete 

WORKDIR /app/

# Create empty lib.rs files.
# In order for cargo init to work, we need to not have a Cargo.toml file. We'll rename Cargo.toml
# to another name and after running `cargo init` we'll override the auto-generated Cargo.toml with
# the original.
RUN mv Cargo.toml _Cargo.toml
RUN for dir in crates/*; do \
    mv $dir/Cargo.toml $dir/_Cargo.toml \
    && cargo init --lib --vcs none $dir \
    && mv -f $dir/_Cargo.toml $dir/Cargo.toml; \
done
RUN mv _Cargo.toml Cargo.toml

# Starting a new stage so that the next layers will be cached if a non-Cargo.toml file is changed.
FROM rust:1.63
WORKDIR /app/

RUN apt update && apt install -y clang

# Copy all the files from the previous stage (which are Cargo.toml and empty lib.rs files).
COPY --from=0 /app .

RUN CARGO_INCREMENTAL=0 cargo build --release --package papyrus_node

# Copy the rest of the files.
COPY . .

RUN touch crates/*/src/lib.rs

RUN CARGO_INCREMENTAL=0 cargo build --release --package papyrus_node --bin papyrus_node

RUN mkdir /app/data

ENTRYPOINT ["/app/target/release/papyrus_node"]
