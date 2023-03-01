apt update
apt install -y curl gcc pkg-config libssl-dev clang npm
curl https://sh.rustup.rs -sSf | sh -s -- -y
. "$HOME/.cargo/env"
rustup install 1.67
rustup install nightly-2022-07-27
npm install -g ganache@7.4.3
