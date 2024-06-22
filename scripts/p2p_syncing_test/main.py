import threading
import time
import sys
from contextlib import contextmanager
import os
import signal
import requests
import subprocess

# Constants
MONITORING_METRICS_URL = "http://localhost:8082/monitoring/metrics"
SLEEP_DURATION_SECONDS = 15

# Global variables to store threads and processes
threads = []
processes = []

# Function to start a node process
def start_node(command):
    process = subprocess.Popen(command, shell=True, preexec_fn=os.setsid, text=True)
    processes.append(process)

# Function to run curl command and extract state marker
def run_curl_and_extract_state_marker(url):
    try:
        response = requests.get(url)
        response.raise_for_status()
        curl_output = response.text
        papyrus_state_marker = extract_state_marker_from_monitoring_output(curl_output)
        return papyrus_state_marker
    except requests.RequestException as e:
        print(f"Error while fetching monitoring metrics: {e}")
        return None

# Context manager to start and manage node processes
@contextmanager
def start_node_processes(client_command, server_command):
    client_thread = threading.Thread(target=start_node, args=(client_command,))
    server_thread = threading.Thread(target=start_node, args=(server_command,))
    threads.extend([client_thread, server_thread])

    # Start threads
    client_thread.start()
    server_thread.start()

    try:
        yield
    finally:
        # Terminate all processes
        for process in processes:
            os.killpg(os.getpgid(process.pid), signal.SIGTERM)
            process.wait()

def extract_state_marker_from_monitoring_output(curl_output):
    lines = curl_output.strip().split("\n")
    print(f"Monitoring metrics: {lines}")
    for line in lines:
        if line.startswith("papyrus_state_marker"):
            papyrus_state_marker = int(line.strip().split()[-1])
            return papyrus_state_marker
    return None


def main():
    if len(sys.argv) != 2:
        print("Usage: python3 scripts/p2p_syncing_test/main.py <BASE_LAYER_NODE_URL>")
        sys.exit(1)

    base_layer_node_url = sys.argv[1]

    client_node_command = (
        f"target/release/papyrus_node --base_layer.node_url {base_layer_node_url} "
        "--config_file scripts/p2p_syncing_test/client_node_config.json"
    )

    server_node_command = (
        f"target/release/papyrus_node --base_layer.node_url {base_layer_node_url} "
        "--config_file scripts/p2p_syncing_test/server_node_config.json"
    )

    with start_node_processes(client_node_command, server_node_command):
        time.sleep(SLEEP_DURATION_SECONDS)

        try:
            curl_output = run_curl_and_extract_state_marker(MONITORING_METRICS_URL)
            if curl_output is not None:
                papyrus_state_marker = extract_state_marker_from_monitoring_output(curl_output)
                print(f"papyrus_state_marker = {papyrus_state_marker}")
                assert (
                    papyrus_state_marker is not None
                ), "Failed to extract state marker value from monitoring output."
                assert (
                    papyrus_state_marker >= 10
                ), f"papyrus_state_marker value is less than 10, papyrus_state_marker {papyrus_state_marker}. Failing CI."
            else:
                print("Failed to fetch monitoring metrics.")
        except Exception as e:
            print(f"An error occurred while running the script: {e}")
        finally:
            # Terminate all threads
            for thread in threads:
                thread._stop()  # Use thread._stop() to terminate a thread abruptly

if __name__ == "__main__":
    main()
