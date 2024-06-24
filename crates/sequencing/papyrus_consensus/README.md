# papyrus-consensus

Milestone 1 - consensus without voting. 

How to run:
1. Start by running any nodes which are validators for block "0", to avoid them missing the proposal.
   1. The block 0 proposer is hard coded as ID 0.

Boot Node - this must be run first:
```
CONSENSUS_VALIDATOR_ID=1 cargo run --package papyrus_node --bin papyrus_node -- --network.#is_none false --base_layer.node_url <ETH_NODE_URL> --storage.db_config.path_prefix <UNIQUE>
```
- This will log `local_peer_id` which is used by other nodes. (Alternatively pass `network.secret_key` to have a fixed peer id).

Other Nodes - the last run should use `CONSENSUS_VALIDATOR_ID=0`.
```
CONSENSUS_VALIDATOR_ID=<UNIQUE> cargo run --package papyrus_node --bin papyrus_node -- --network.#is_none false --network.tcp_port <UNIQUE> --network.bootstrap_peer_multiaddr.#is_none false --rpc.server_address 127.0.0.1:<UNIQUE> --monitoring_gateway.server_address 127.0.0.1:<UNIQUE> --storage.db_config.path_prefix <UNIQUE> --base_layer.node_url <ETH_NODE_URL> --network.bootstrap_peer_multiaddr /ip4/127.0.0.1/tcp/10000/p2p/<BOOT_NODE_PEER_ID>
```

UNIQUE - a value unique among all nodes running locally.
