use super::protobuf;

impl protobuf::BlockHeadersResponse {
    pub fn is_fin(&self) -> bool {
        self.header_message.as_ref().map_or(false, |response| {
            matches!(
                response,
                crate::messages::proto::p2p::proto::block_headers_response::HeaderMessage::Fin(_)
            )
        })
    }
}
