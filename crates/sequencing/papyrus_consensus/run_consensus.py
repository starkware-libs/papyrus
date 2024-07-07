import subprocess
import time
import os
import signal
import argparse
import tempfile

SECRET_KEY = "0xabcdabcdabcdabcdabcdabcdabcdabcdabcdabcdabcdabcdabcdabcdabcdabcd"
BOOT_NODE_PEER_ID = "12D3KooWDFYi71juk6dYWo3UDvqs5gAzGDc124LSvcR5d187Tdvi"


def run_command(command):
    return subprocess.run(command, shell=True, check=True)


def run_parallel_commands(commands, runtime):
    processes = []
    for command in commands:
        process = subprocess.Popen(command, shell=True, preexec_fn=os.setsid)
        processes.append(process)

    def cleanup():
        for process in processes:
            os.killpg(os.getpgid(process.pid), signal.SIGTERM)
            process.wait()

    try:
        time.sleep(runtime)
    except KeyboardInterrupt:
        print("\nCtrl+C pressed: Terminating subprocesses...")
    finally:
        cleanup()


def convert_ansi_to_txt(temp_dir, num_validators):
    for i in range(num_validators):
        ansi_file = os.path.join(temp_dir, f"validator{i}.ansi")
        txt_file = os.path.join(temp_dir, f"validator{i}.txt")
        command = f"sed 's/\\x1b\\[[0-9;]*[a-zA-Z]//g' {ansi_file} > {txt_file}"
        run_command(command)


def main(base_layer_node_url, num_validators, runtime):
    # Run cargo build
    print("Running cargo build...")
    run_command("cargo build -r")

    # Clean and create data directories
    for i in range(num_validators):
        data_dir = f"data{i}"
        if os.path.exists(data_dir):
            subprocess.run(f"rm -rf {data_dir}", shell=True)
        os.makedirs(data_dir)

    temp_dir = tempfile.mkdtemp()
    print(f"Output files will be stored in: {temp_dir}")

    validator_commands = []
    # Ensure validator 1 runs first
    command = (
        f"RUST_LOG=papyrus_consensus=debug,papyrus=info "
        f"target/release/papyrus_node --network.#is_none false "
        f"--base_layer.node_url {base_layer_node_url} "
        f"--network.secret_key {SECRET_KEY} "
        f"--storage.db_config.path_prefix ./data1 "
        f"--consensus.#is_none false --consensus.validator_id 0x1 "
        f"--consensus.num_of_validators {num_validators} "
        f"> {temp_dir}/validator1.ansi"
    )
    validator_commands.append(command)

    # Add other validators
    for i in range(2, num_validators):
        base_port = 10000 + i * 3  # Ensure unique port ranges for each validator
        command = (
            f"RUST_LOG=papyrus_consensus=debug,papyrus=info "
            f"target/release/papyrus_node --network.#is_none false "
            f"--base_layer.node_url {base_layer_node_url} "
            f"--storage.db_config.path_prefix ./data{i} "
            f"--consensus.#is_none false --consensus.validator_id 0x{i} "
            f"--consensus.num_of_validators {num_validators} "
            f"--network.tcp_port {base_port} "
            f"--rpc.server_address 127.0.0.1:{base_port + 1} "
            f"--monitoring_gateway.server_address 127.0.0.1:{base_port + 2} "
            f"--network.bootstrap_peer_multiaddr.#is_none false "
            f"--network.bootstrap_peer_multiaddr /ip4/127.0.0.1/tcp/10000/p2p/{BOOT_NODE_PEER_ID} "
            f"> {temp_dir}/validator{i}.ansi"
        )
        validator_commands.append(command)

    # Ensure validator 0 runs last
    base_port = 10000 + num_validators * 3
    command = (
        f"RUST_LOG=papyrus_consensus=debug,papyrus=info "
        f"target/release/papyrus_node --network.#is_none false "
        f"--base_layer.node_url {base_layer_node_url} "
        f"--storage.db_config.path_prefix ./data0 "
        f"--consensus.#is_none false --consensus.validator_id 0x0 "
        f"--consensus.num_of_validators {num_validators} "
        f"--network.tcp_port {base_port + 5} "
        f"--rpc.server_address 127.0.0.1:{base_port + 1} "
        f"--monitoring_gateway.server_address 127.0.0.1:{base_port + 2} "
        f"--network.bootstrap_peer_multiaddr.#is_none false "
        f"--network.bootstrap_peer_multiaddr /ip4/127.0.0.1/tcp/10000/p2p/{BOOT_NODE_PEER_ID} "
        f"> {temp_dir}/validator0.ansi"
    )
    validator_commands.append(command)

    # Run validator commands in parallel and manage runtime
    print("Running validators...")
    run_parallel_commands(validator_commands, runtime)
    convert_ansi_to_txt(temp_dir, num_validators)
    print("Simulation complete.")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Run Papyrus Node simulation.")
    parser.add_argument("--base_layer_node_url", required=True)
    parser.add_argument("--num_validators", type=int, required=True)
    parser.add_argument("--runtime", type=int, required=True)

    args = parser.parse_args()
    main(args.base_layer_node_url, args.num_validators, args.runtime)
