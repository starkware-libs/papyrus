use papyrus_protobuf::sync::{Direction, Query};

use super::{DBExecutorError, QueryId};

pub(crate) fn calculate_block_number(
    query: &Query,
    start_block: u64,
    read_blocks_counter: u64,
    query_id: QueryId,
) -> Result<u64, DBExecutorError> {
    let direction_factor: i128 = match query.direction {
        Direction::Forward => 1,
        Direction::Backward => -1,
    };
    let step = u64::try_from(query.step).expect("Failed converting usize to u64");
    // TODO(shahak): Fix this code.
    let blocks_delta: i128 = direction_factor * (step * read_blocks_counter) as i128;
    let block_number: i128 = start_block as i128 + blocks_delta;
    if block_number < 0 || block_number > u64::MAX as i128 {
        return Err(DBExecutorError::BlockNumberOutOfRange {
            query: query.clone(),
            counter: read_blocks_counter,
            query_id,
        });
    }
    Ok(block_number as u64)
}
