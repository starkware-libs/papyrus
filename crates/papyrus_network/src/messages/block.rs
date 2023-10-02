use super::protobuf;

impl protobuf::BlockHeadersResponse {
    pub fn is_fin(&self) -> bool {
        self.part.last().map_or(false, |response| {
            matches!(
                response,
                crate::messages::proto::p2p::proto::BlockHeadersResponsePart{header_message: Some(crate::messages::proto::p2p::proto::block_headers_response_part::HeaderMessage::Fin(_))}
            )
        })
    }
}
