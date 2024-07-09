use std::sync::OnceLock;

/// The central marker is the first block number that doesn't exist yet.
pub const PAPYRUS_CENTRAL_BLOCK_MARKER: &str = "papyrus_central_block_marker";

/// The header marker is the first block number for which the node does not have a header.
pub const PAPYRUS_HEADER_MARKER: &str = "papyrus_header_marker";

/// The body marker is the first block number for which the node does not have a body.
pub const PAPYRUS_BODY_MARKER: &str = "papyrus_body_marker";

/// The state marker is the first block number for which the node does not have a state body.
pub const PAPYRUS_STATE_MARKER: &str = "papyrus_state_marker";

/// The compiled class marker is the first block number for which the node does not have all of the
/// corresponding compiled classes.
pub const PAPYRUS_COMPILED_CLASS_MARKER: &str = "papyrus_compiled_class_marker";

/// The base layer marker is the first block number for which the node does not guarantee L1
/// finality.
pub const PAPYRUS_BASE_LAYER_MARKER: &str = "papyrus_base_layer_marker";

/// The latency, in seconds, between a block timestamp (as state in its header) and the time the
/// node stores the header.
pub const PAPYRUS_HEADER_LATENCY_SEC: &str = "papyrus_header_latency";

/// The number of peers this node is connected to.
pub const PAPYRUS_NUM_CONNECTED_PEERS: &str = "papyrus_num_connected_peers";

/// The number of active sessions this peer has in which it sends data.
pub const PAPYRUS_NUM_ACTIVE_INBOUND_SESSIONS: &str = "papyrus_num_active_inbound_sessions";

/// The number of active sessions this peer has in which it requests data.
pub const PAPYRUS_NUM_ACTIVE_OUTBOUND_SESSIONS: &str = "papyrus_num_active_outbound_sessions";

// TODO: consider making this value non static and add a way to change this while the app is
// running. e.g via a monitoring endpoint.
/// Global variable set by the main config to enable collecting profiling metrics.
pub static COLLECT_PROFILING_METRICS: OnceLock<bool> = OnceLock::new();

/// The height most recently decided by consensus.
pub const PAPYRUS_CONSENSUS_HEIGHT: &str = "papyrus_consensus_height";
