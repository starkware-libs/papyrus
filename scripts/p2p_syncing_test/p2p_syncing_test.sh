#!/bin/bash

# Constants
MONITORING_METRICS_URL="http://localhost:8082/monitoring/metrics"
SLEEP_DURATION_SECONDS=15

# Function to start node processes
start_node_processes() {
    local client_command=$1
    local server_command=$2

    eval "$client_command" &
    client_pid=$!

    eval "$server_command" &
    server_pid=$!

    echo "Client PID: $client_pid"
    echo "Server PID: $server_pid"

    sleep $SLEEP_DURATION_SECONDS

    # Run curl and check the state marker
    curl_output=$(curl -s -X GET "$MONITORING_METRICS_URL")

    # Extract the numeric value after papyrus_state_marker
    papyrus_state_marker=$(echo "$curl_output" | grep -oP 'papyrus_state_marker \K\d+')

    echo "papyrus_state_marker = $papyrus_state_marker"

    if [[ -z "$papyrus_state_marker" ]]; then
        echo "Failed to extract a valid state marker value from monitoring output."
        cleanup
        exit 1
    fi

    if (( papyrus_state_marker < 10 )); then
        echo "papyrus_state_marker value is less than 10, papyrus_state_marker $papyrus_state_marker. Failing CI."
        cleanup
        exit 1
    fi
    cleanup
}
# Function to ensure cleanup on script exit
cleanup() {
    echo "Cleaning up..."
    echo "Client PID: $client_pid"
    echo "Server PID: $server_pid"
    pgrep -P $$
    pkill -P $client_pid
    pkill -P $server_pid
    kill -KILL "$client_pid"
    kill -KILL "$server_pid"
}

# Main function
main() {
    if [[ $# -ne 1 ]]; then
        echo "Usage: $0 <BASE_LAYER_NODE_URL>"
        exit 1
    fi

    base_layer_node_url=$1

    client_node_command="target/release/papyrus_node --base_layer.node_url $base_layer_node_url --config_file scripts/p2p_syncing_test/client_node_config.json"
    server_node_command="target/release/papyrus_node --base_layer.node_url $base_layer_node_url --config_file scripts/p2p_syncing_test/server_node_config.json"

    # Set trap to ensure cleanup on script exit
    # trap cleanup EXIT

    start_node_processes "$client_node_command" "$server_node_command"
}

# Call main with all arguments
main "$@"