use std::task::Poll;

use futures::{SinkExt, StreamExt};

use crate::executor::db_executor::DbExecutor;
use crate::streamed_data_protocol::InboundSessionId;
use crate::{BlockQuery, BlockResult};

#[tokio::test]
async fn receive_queries_from_multiple_senders() {
    let mut executor = DbExecutor::new();
    let mut query_sender1 = executor.get_query_sender();
    let block_query1 =
        BlockQuery { outbound_session_id: InboundSessionId { value: 1 }, ..Default::default() };
    let mut query_sender2 = executor.get_query_sender();
    let block_query2 =
        BlockQuery { outbound_session_id: InboundSessionId { value: 2 }, ..Default::default() };

    tokio::join!(
        async move {
            query_sender1.send(block_query1).await.unwrap();
        },
        async move {
            query_sender2.send(block_query2).await.unwrap();
        },
    );

    let query_receiver = executor.get_query_receiver();
    if let Ok(Some(query_res)) = query_receiver.try_next() {
        assert_eq!(query_res, block_query1);
    }
    if let Ok(Some(query_res)) = query_receiver.try_next() {
        assert_eq!(query_res, block_query2);
    }
}

#[test]
fn block_receiver_can_be_taken_once() {
    let mut executor = DbExecutor::new();
    let _ = executor.get_blocks_data_receiver();
    assert_matches::assert_matches!(executor.get_blocks_data_receiver(), None);
}

#[tokio::test]
async fn terminating_query_receiver_respond_with_status_then_terminate() {
    let mut executor = DbExecutor::new();

    // put some db_ops in the queue to make sure the executor is not terminated
    let db_ops = executor.get_db_ops();
    db_ops.push(Box::pin(async move { BlockResult::default() }));

    // terminate the query receiver
    let query_receiver = executor.get_query_receiver();
    query_receiver.close();

    // first poll should return status
    match executor
        .poll_next_unpin(&mut futures::task::Context::from_waker(futures::task::noop_waker_ref()))
    {
        Poll::Ready(Some(executor_status)) => {
            println!("{:?}", executor_status);
            assert!(executor_status.query_receiver_terminated);
        }
        _ => panic!("Expected to receive status"),
    }
    // once all db_ops are consumed (after one poll in this case) return terminate stream
    assert_matches::assert_matches!(
        executor.poll_next_unpin(&mut futures::task::Context::from_waker(
            futures::task::noop_waker_ref()
        )),
        Poll::Ready(None)
    );
}

#[tokio::test]
#[ignore = "Implement query processing first"]
async fn process_query() {}
