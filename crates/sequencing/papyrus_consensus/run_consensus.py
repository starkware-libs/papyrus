import subprocess
import time
import os
import signal
import argparse
import tempfile
import socket
from contextlib import closing


# The SECRET_KEY is used for building the BOOT_NODE_PEER_ID, so they are coupled and must be used together.
SECRET_KEY = "0xabcdabcdabcdabcdabcdabcdabcdabcdabcdabcdabcdabcdabcdabcdabcdabcd"
BOOT_NODE_PEER_ID = "12D3KooWDFYi71juk6dYWo3UDvqs5gAzGDc124LSvcR5d187Tdvi"


def find_free_port():
    with closing(socket.socket(socket.AF_INET, socket.SOCK_STREAM)) as s:
        s.bind(("", 0))
        s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEPORT, 1)
        return s.getsockname()[1]


def run_command(command):
    return subprocess.run(command, shell=True, check=True)


def run_parallel_commands(commands, duration):
    processes = []
    for command in commands:
        process = subprocess.Popen(command, shell=True, preexec_fn=os.setsid)
        processes.append(process)

    try:
        time.sleep(duration)
    except KeyboardInterrupt:
        print("\nCtrl+C pressed: Terminating subprocesses...")
    finally:
        for process in processes:
            os.killpg(os.getpgid(process.pid), signal.SIGINT)
            process.wait()


def peernode_command(base_layer_node_url, temp_dir, num_validators, i):
    return (
        f"RUST_LOG=papyrus_consensus=debug,papyrus=info "
        f"target/release/papyrus_node --network.#is_none false "
        f"--base_layer.node_url {base_layer_node_url} "
        f"--storage.db_config.path_prefix {temp_dir}/data{i} "
        f"--consensus.#is_none false --consensus.validator_id 0x{i} "
        f"--consensus.num_validators {num_validators} "
        f"--network.tcp_port {find_free_port()} "
        f"--rpc.server_address 127.0.0.1:{find_free_port()} "
        f"--monitoring_gateway.server_address 127.0.0.1:{find_free_port()} "
        f"--network.bootstrap_peer_multiaddr.#is_none false "
        f"--network.bootstrap_peer_multiaddr /ip4/127.0.0.1/tcp/10000/p2p/{BOOT_NODE_PEER_ID} "
        # Use sed to strip special formatting characters
        f"| sed -r 's/\\x1B\\[[0-9;]*[mK]//g' > {temp_dir}/validator{i}.txt"
    )


def main(base_layer_node_url, num_validators, duration):
    assert num_validators >= 2, "At least 2 validators are required for the simulation."
    # Building the Papyrus Node package assuming its output will be located in the papyrus target directory.
    print("Running cargo build...")
    run_command("cargo build --release --package papyrus_node")

    temp_dir = tempfile.mkdtemp()
    print(f"Output files will be stored in: {temp_dir}")

    # Create data directories
    for i in range(num_validators):
        data_dir = os.path.join(temp_dir, f"data{i}")
        os.makedirs(data_dir)

    # Validators are started in a specific order to ensure proper network formation:
    # 1. The bootnode (validator 1) is started first for network peering.
    # 2. Validators 2+ are started next to join the network through the bootnode.
    # 3. Validator 0, which is the proposer, is started last so the validators don't miss the proposals.

    validator_commands = []
    # Ensure validator 1 runs first
    bootnode_command = (
        f"RUST_LOG=papyrus_consensus=debug,papyrus=info "
        f"target/release/papyrus_node --network.#is_none false "
        f"--base_layer.node_url {base_layer_node_url} "
        f"--network.secret_key {SECRET_KEY} "
        f"--storage.db_config.path_prefix {temp_dir}/data1 "
        f"--consensus.#is_none false --consensus.validator_id 0x1 "
        f"--consensus.num_validators {num_validators} "
        # Use sed to strip special formatting characters
        f"| sed -r 's/\\x1B\\[[0-9;]*[mK]//g' > {temp_dir}/validator1.txt"
    )
    validator_commands.append(bootnode_command)

    # Add other validators
    validator_commands.extend(
        peernode_command(base_layer_node_url, temp_dir, num_validators, i)
        for i in range(2, num_validators)
    )
    # Ensure validator 0 runs last
    validator_commands.append(peernode_command(base_layer_node_url, temp_dir, num_validators, 0))

    # Run validator commands in parallel and manage duration time
    print("Running validators...")
    run_parallel_commands(validator_commands, duration)
    print("Simulation complete.")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Run Papyrus Node simulation.")
    parser.add_argument("--base_layer_node_url", required=True)
    parser.add_argument("--num_validators", type=int, required=True)
    parser.add_argument("--duration", type=int, required=True)

    args = parser.parse_args()
    main(args.base_layer_node_url, args.num_validators, args.duration)
