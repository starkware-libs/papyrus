import subprocess
import time
import sys
from contextlib import contextmanager

# Constants
MONITORING_METRICS_URL = "http://localhost:8082/monitoring/metrics"
SLEEP_DURATION_SECONDS = 15


@contextmanager
def start_node_processes(client_command, server_command):
    client_process = subprocess.Popen(client_command, text=True)
    server_process = subprocess.Popen(server_command, text=True)

    try:
        yield client_process, server_process
    finally:
        client_process.terminate()
        server_process.terminate()
        client_process.wait()
        server_process.wait()


def run_curl_on_client_node_monitoring():
    return subprocess.check_output(["curl", "-X", "GET", MONITORING_METRICS_URL], text=True)


def extract_state_marker_from_monitoring_output(curl_output):
    lines = curl_output.strip().split("\n")
    for line in lines:
        if line.startswith("papyrus_state_marker"):
            papyrus_state_marker = int(line.strip().split()[-1])
            return papyrus_state_marker
    return None


def main():
    if len(sys.argv) != 2:
        print("Usage: python3 p2p_syncing_test.py <BASE_LAYER_NODE_URL>")
        sys.exit(1)

    base_layer_node_url = sys.argv[1]

    client_node_command = [
        "target/release/papyrus_node",
        "--base_layer.node_url",
        base_layer_node_url,
        "--config_file",
        "scripts/p2p_syncing_test/client_node_config.json",
    ]

    server_node_command = [
        "target/release/papyrus_node",
        "--base_layer.node_url",
        base_layer_node_url,
        "--config_file",
        "scripts/p2p_syncing_test/server_node_config.json",
    ]

    with start_node_processes(client_node_command, server_node_command):
        time.sleep(SLEEP_DURATION_SECONDS)

        try:
            curl_output = run_curl_on_client_node_monitoring()
            papyrus_state_marker = extract_state_marker_from_monitoring_output(curl_output)
            print(papyrus_state_marker)
            if papyrus_state_marker is None:
                print("Failed to extract state marker value from monitoring output.")
                sys.exit(1)

            if papyrus_state_marker < 10:
                print(
                    f"papyrus_state_marker value is less than 10, papyrus_state_marker {papyrus_state_marker}. Failing CI."
                )
                sys.exit(1)

        except subprocess.CalledProcessError as e:
            print(f"An error occurred while running a command: {e}")
            sys.exit(1)


if __name__ == "__main__":
    main()
