use super::protobuf;

impl protobuf::BlockHeadersResponse {
    pub fn is_fin(&self) -> bool {
        self.part.last().map_or(false, |response| {
            matches!(
                response,
                protobuf::BlockHeadersResponsePart {
                    header_message: Some(
                        protobuf::block_headers_response_part::HeaderMessage::Fin(_)
                    )
                }
            )
        })
    }
}
