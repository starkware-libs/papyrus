pub(crate) struct PeerManager;

// TODO(Nevo): define types for peer request
#[derive(Default)]
pub(crate) struct PeerRequest {
    pub peer_id: String,
    pub request: String,
}

impl PeerManager {
    pub(crate) fn split_request_and_assign_peers(&self, query: String) -> Vec<PeerRequest> {
        vec![Default::default()]
    }
}
