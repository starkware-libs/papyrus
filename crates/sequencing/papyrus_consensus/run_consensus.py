import subprocess
import time
import os
import signal
import argparse
import tempfile
import socket
from contextlib import closing
import threading

# The SECRET_KEY is used for building the BOOT_NODE_PEER_ID, so they are coupled and must be used together.
SECRET_KEY = "0xabcdabcdabcdabcdabcdabcdabcdabcdabcdabcdabcdabcdabcdabcdabcdabcd"
BOOT_NODE_PEER_ID = "12D3KooWDFYi71juk6dYWo3UDvqs5gAzGDc124LSvcR5d187Tdvi"


class Node:
    def __init__(self, validator_id, monitoring_gateway_server_port, cmd, stagnation_threshold):
        self.validator_id = validator_id
        self.monitoring_gateway_server_port = monitoring_gateway_server_port
        self.cmd = cmd
        self.process = None
        self.last_height = (None, None)  # (timestamp, height)
        self.stagnation_threshold = stagnation_threshold

    def start(self):
        self.process = subprocess.Popen(self.cmd, shell=True, preexec_fn=os.setsid)

    def stop(self):
        if self.process:
            os.killpg(os.getpgid(self.process.pid), signal.SIGINT)
            self.process.wait()

    def get_height(self, start_time):
        port = self.monitoring_gateway_server_port
        command = f"curl -s -X GET http://localhost:{port}/monitoring/metrics | grep -oP 'papyrus_consensus_height \\K\\d+'"
        result = subprocess.run(command, shell=True, capture_output=True, text=True)
        current_height = result.stdout.strip()

        current_time = time.time() - start_time
        last_time, last_height = self.last_height
        if last_height is not None and current_height == last_height:
            if current_time - last_time >= self.stagnation_threshold:
                print(
                    f"Consensus stops. Validator {self.validator_id} height {current_height} hasn't changed for {self.stagnation_threshold} seconds. Exiting..."
                )
                return None
        else:
            self.last_height = (
                (current_time, current_height) if current_height != "" else (None, None)
            )

        return current_height


def find_free_port():
    with closing(socket.socket(socket.AF_INET, socket.SOCK_STREAM)) as s:
        s.bind(("", 0))
        s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEPORT, 1)
        return s.getsockname()[1]


def run_command(command):
    return subprocess.run(command, shell=True, check=True)


def run_parallel_commands(nodes, duration):
    for node in nodes:
        node.start()

    def periodic_height_check():
        start_time = time.time()
        i = 1
        while True:
            time.sleep(10)
            print(f"\nAfter {i * 10} seconds:")
            for node in nodes:
                height = node.get_height(start_time)
                if height is None:
                    os.kill(os.getpid(), signal.SIGINT)
                    return
                print(f"Validator {node.validator_id} height: {height}")

            i += 1

    threading.Thread(target=periodic_height_check, daemon=True).start()

    try:
        if duration is not None:
            time.sleep(duration)
        else:
            while True:
                pass
    except KeyboardInterrupt:
        print("\nTerminating subprocesses...")
    finally:
        for node in nodes:
            node.stop()


def peernode_command(base_layer_node_url, temp_dir, num_validators, i, stagnation_threshold):
    monitoring_gateway_server_port = find_free_port()
    cmd = (
        f"RUST_LOG=papyrus_consensus=debug,papyrus=info "
        f"target/release/papyrus_node --network.#is_none false "
        f"--base_layer.node_url {base_layer_node_url} "
        f"--storage.db_config.path_prefix {temp_dir}/data{i} "
        f"--consensus.#is_none false --consensus.validator_id 0x{i} "
        f"--consensus.num_validators {num_validators} "
        f"--network.tcp_port {find_free_port()} "
        f"--rpc.server_address 127.0.0.1:{find_free_port()} "
        f"--monitoring_gateway.server_address 127.0.0.1:{monitoring_gateway_server_port} "
        f"--network.bootstrap_peer_multiaddr.#is_none false "
        f"--network.bootstrap_peer_multiaddr /ip4/127.0.0.1/tcp/10000/p2p/{BOOT_NODE_PEER_ID} "
        f"--collect_metrics true"
        # Use sed to strip special formatting characters
        f"| sed -r 's/\\x1B\\[[0-9;]*[mK]//g' > {temp_dir}/validator{i}.txt"
    )
    return Node(
        validator_id=i,
        monitoring_gateway_server_port=monitoring_gateway_server_port,
        cmd=cmd,
        stagnation_threshold=stagnation_threshold,
    )


def main(base_layer_node_url, num_validators, stagnation_threshold, duration):
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

    nodes = []
    # Ensure validator 1 runs first
    bootnode_command = (
        f"RUST_LOG=papyrus_consensus=debug,papyrus=info "
        f"target/release/papyrus_node --network.#is_none false "
        f"--base_layer.node_url {base_layer_node_url} "
        f"--network.secret_key {SECRET_KEY} "
        f"--storage.db_config.path_prefix {temp_dir}/data1 "
        f"--consensus.#is_none false --consensus.validator_id 0x1 "
        f"--consensus.num_validators {num_validators} "
        f"--monitoring_gateway.server_address 127.0.0.1:8081 "
        f"--collect_metrics true"
        # Use sed to strip special formatting characters
        f"| sed -r 's/\\x1B\\[[0-9;]*[mK]//g' > {temp_dir}/validator1.txt"
    )
    nodes.append(
        Node(
            validator_id=1,
            monitoring_gateway_server_port=8081,
            cmd=bootnode_command,
            stagnation_threshold=stagnation_threshold,
        )
    )

    # Add other validators
    nodes.extend(
        peernode_command(base_layer_node_url, temp_dir, num_validators, i, stagnation_threshold)
        for i in range(2, num_validators)
    )
    # Ensure validator 0 runs last
    nodes.append(
        peernode_command(base_layer_node_url, temp_dir, num_validators, 0, stagnation_threshold)
    )

    # Run validator commands in parallel and manage duration time
    print("Running validators...")
    run_parallel_commands(nodes, duration)
    print("Simulation complete.")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Run Papyrus Node simulation.")
    parser.add_argument("--base_layer_node_url", required=True)
    parser.add_argument("--num_validators", type=int, required=True)
    parser.add_argument(
        "--stagnation_threshold",
        type=int,
        required=False,
        default=30,
        help="Time in seconds to check for height stagnation.",
    )
    parser.add_argument("--duration", type=int, required=False, default=None)

    args = parser.parse_args()
    main(args.base_layer_node_url, args.num_validators, args.stagnation_threshold, args.duration)
