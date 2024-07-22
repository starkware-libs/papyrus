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

MONITORING_PERIOD = 10


class Node:
    def __init__(self, validator_id, monitoring_gateway_server_port, cmd):
        self.validator_id = validator_id
        self.monitoring_gateway_server_port = monitoring_gateway_server_port
        self.cmd = cmd
        self.process = None
        self.height_and_timestamp = (None, None)  # (height, timestamp)

    def start(self):
        self.process = subprocess.Popen(self.cmd, shell=True, preexec_fn=os.setsid)

    def stop(self):
        if self.process:
            os.killpg(os.getpgid(self.process.pid), signal.SIGINT)
            self.process.wait()

    def get_height(self):
        port = self.monitoring_gateway_server_port
        command = f"curl -s -X GET http://localhost:{port}/monitoring/metrics | grep -oP 'papyrus_consensus_height \\K\\d+'"
        result = subprocess.run(command, shell=True, capture_output=True, text=True)
        # returns the most recently decided height, or None if node is not ready or consensus has not yet reached any height.
        return int(result.stdout) if result.stdout else None

    def check_height(self):
        height = self.get_height()
        if self.height_and_timestamp[0] != height:
            if self.height_and_timestamp[0] is not None and height is not None:
                assert height > self.height_and_timestamp[0], "Height should be increasing."
            self.height_and_timestamp = (height, time.time())

        return self.height_and_timestamp


def find_free_port():
    with closing(socket.socket(socket.AF_INET, socket.SOCK_STREAM)) as s:
        s.bind(("", 0))
        s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEPORT, 1)
        return s.getsockname()[1]


BOOTNODE_TCP_PORT = find_free_port()


# Returns if the simulation should exit.
def monitor_simulation(nodes, start_time, duration, stagnation_timeout):
    curr_time = time.time()
    if duration is not None and duration < (curr_time - start_time):
        return True
    stagnated_nodes = []
    for node in nodes:
        (height, last_update) = node.check_height()
        print(f"Node: {node.validator_id}, height: {height}")
        if height is not None and (curr_time - last_update) > stagnation_timeout:
            stagnated_nodes.append(node.validator_id)
    if stagnated_nodes:
        print(f"Nodes {stagnated_nodes} have stagnated. Exiting simulation.")
        return True
    return False


def run_simulation(nodes, duration, stagnation_timeout):
    for node in nodes:
        node.start()

    start_time = time.time()
    try:
        while True:
            time.sleep(MONITORING_PERIOD)
            print(f"\nTime elapsed: {time.time() - start_time}s")
            should_exit = monitor_simulation(nodes, start_time, duration, stagnation_timeout)
            if should_exit:
                break
    except KeyboardInterrupt:
        print("\nTerminating subprocesses...")
    finally:
        for node in nodes:
            node.stop()


def build_node(base_layer_node_url, temp_dir, num_validators, i):
    is_bootstrap = i == 1
    tcp_port = BOOTNODE_TCP_PORT if is_bootstrap else find_free_port()
    monitoring_gateway_server_port = find_free_port()

    cmd = (
        f"RUST_LOG=papyrus_consensus=debug,papyrus=info "
        f"target/release/papyrus_node --network.#is_none false "
        f"--base_layer.node_url {base_layer_node_url} "
        f"--storage.db_config.path_prefix {temp_dir}/data{i} "
        f"--consensus.#is_none false --consensus.validator_id 0x{i} "
        f"--consensus.num_validators {num_validators} "
        f"--network.tcp_port {tcp_port} "
        f"--rpc.server_address 127.0.0.1:{find_free_port()} "
        f"--monitoring_gateway.server_address 127.0.0.1:{monitoring_gateway_server_port} "
        f"--collect_metrics true "
    )

    if is_bootstrap:
        cmd += (
            f"--network.secret_key {SECRET_KEY} "
            + f"| sed -r 's/\\x1B\\[[0-9;]*[mK]//g' > {temp_dir}/validator{i}.txt"
        )

    else:
        cmd += (
            f"--network.bootstrap_peer_multiaddr.#is_none false "
            f"--network.bootstrap_peer_multiaddr /ip4/127.0.0.1/tcp/{BOOTNODE_TCP_PORT}/p2p/{BOOT_NODE_PEER_ID} "
            + f"| sed -r 's/\\x1B\\[[0-9;]*[mK]//g' > {temp_dir}/validator{i}.txt"
        )

    return Node(
        validator_id=i,
        monitoring_gateway_server_port=monitoring_gateway_server_port,
        cmd=cmd,
    )


def build_all_nodes(base_layer_node_url, temp_dir, num_validators):
    # Validators are started in a specific order to ensure proper network formation:
    # 1. The bootnode (validator 1) is started first for network peering.
    # 2. Validators 2+ are started next to join the network through the bootnode.
    # 3. Validator 0, which is the proposer, is started last so the validators don't miss the proposals.
    nodes = []

    nodes.append(build_node(base_layer_node_url, temp_dir, num_validators, 1)) # Bootstrap

    for i in range(2, num_validators):
        nodes.append(build_node(base_layer_node_url, temp_dir, num_validators, i))

    nodes.append(build_node(base_layer_node_url, temp_dir, num_validators, 0)) # Proposer

    return nodes


def main(base_layer_node_url, num_validators, stagnation_threshold, duration):
    assert num_validators >= 2, "At least 2 validators are required for the simulation."
    # Building the Papyrus Node package assuming its output will be located in the papyrus target directory.
    print("Running cargo build...")
    subprocess.run("cargo build --release --package papyrus_node", shell=True, check=True)

    temp_dir = tempfile.mkdtemp()
    print(f"Output files will be stored in: {temp_dir}")

    # Create data directories
    for i in range(num_validators):
        data_dir = os.path.join(temp_dir, f"data{i}")
        os.makedirs(data_dir)

    nodes = build_all_nodes(base_layer_node_url, temp_dir, num_validators)

    # Run validator commands in parallel and manage duration time
    print("Running validators...")
    run_simulation(nodes, duration, stagnation_threshold)
    print(f"Output files were stored in: {temp_dir}")
    print("Simulation complete.")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Run Papyrus Node simulation.")
    parser.add_argument("--base_layer_node_url", required=True)
    parser.add_argument("--num_validators", type=int, required=True)
    parser.add_argument(
        "--stagnation_threshold",
        type=int,
        required=False,
        default=60,
        help="Time in seconds to check for height stagnation.",
    )
    parser.add_argument("--duration", type=int, required=False, default=None)

    args = parser.parse_args()
    main(args.base_layer_node_url, args.num_validators, args.stagnation_threshold, args.duration)
