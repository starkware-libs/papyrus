import subprocess
import time
import json
import signal
import os
import shutil
import sys

def create_directories():
    dirs = ['data', 'data2']
    
    for dir in dirs:
        if os.path.exists(dir):
            shutil.rmtree(dir)
        os.makedirs(dir)

def run_cargo_command(command):
    try:
        process = subprocess.Popen(command, shell=True, preexec_fn=os.setsid, text=True)
        return process
    except Exception as e:
        print(f"Failed to start command: {command}\nError: {e}")
        sys.exit(1)

def run_curl_command():
    result = subprocess.run("curl -X GET http://localhost:8082/monitoring/metrics", shell=True, capture_output=True, text=True)
    return result.stdout

def parse_output(curl_output):
    try:
        lines = curl_output.strip().split('\n')
        for line in lines:
            if line.startswith("papyrus_state_marker"):
                papyrus_state_marker = int(line.strip().split()[-1])
                return papyrus_state_marker
        return None
    except Exception as e:
        print(f"Error parsing curl -X GET http://localhost:8082/monitoring/metrics output: {e}")
        sys.exit(1)

def terminate_process_group(pgid):
    try:
        os.killpg(pgid, signal.SIGTERM)
    except OSError:
        pass

def main():
    if len(sys.argv) != 2:
        print("Usage: python3 p2p_syncing_test.py <BASE_LAYER_NODE_URL>")
        sys.exit(1)
    
    base_layer_node_url = sys.argv[1]

    create_directories()
    
    cargo_command_1 = f"cargo run --release --package papyrus_node --bin papyrus_node -- --base_layer.node_url {base_layer_node_url} --config_file scripts/ci_papyrus_mainnet_config_p2p_sync_receiver.json"
    cargo_command_2 = f"cargo run --release --package papyrus_node --bin papyrus_node -- --base_layer.node_url {base_layer_node_url} --config_file scripts/ci_papyrus_mainnet_config_p2p_sync_source.json"
  
    # run the commands in parallel
    process1 = run_cargo_command(cargo_command_1)
    process2 = run_cargo_command(cargo_command_2)
    
    time.sleep(1000)
    
    curl_output = run_curl_command()
    
    papyrus_state_marker = parse_output(curl_output)
    if papyrus_state_marker is None:
        print("Failed to parse state marker value from Prometheus output.")
        sys.exit(1)
    
    if papyrus_state_marker < 10:
        print(f"papyrus_state_marker value is less than 10, papyrus_state_marker {papyrus_state_marker}. Failing CI.")
        sys.exit(1)

    terminate_process_group(os.getpgid(process1.pid))
    terminate_process_group(os.getpgid(process2.pid))

if __name__ == "__main__":
    main()
