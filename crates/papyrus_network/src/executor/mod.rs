use futures::Stream;

mod db_executor;

// configure which executors should be active
pub struct ExecutorsConfig {
    db_executor: bool,
}

// TODO(Nevo): implement status
// in general executors should run forever, and should publish a status to help manage the load
// e.g. db_executor should publish an indication of the DB load (e.g. number of pending requests)
pub(crate) struct ExecutorsStatus {}

pub(crate) struct Executors {
    db_executor: Option<db_executor::DbExecutor>,
}

impl Executors {
    pub(crate) fn new(config: ExecutorsConfig) -> Self {
        let db_executor = if config.db_executor { Some(db_executor::DbExecutor {}) } else { None };
        Self { db_executor }
    }

    pub(crate) fn get_db_executor(&self) -> &Option<db_executor::DbExecutor> {
        &self.db_executor
    }
}

impl Stream for Executors {
    type Item = ExecutorsStatus;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        // TOSO(Nevo): should poll all executors and return a combined status
        todo!()
    }
}
