#[cfg(test)]
#[path = "retry_utils_test.rs"]
mod retry_utils_test;

use std::fmt::Debug;
use std::iter::Take;
use std::time::Duration;

use log::debug;
use serde::{Deserialize, Serialize};
use tokio_retry::strategy::ExponentialBackoff;
use tokio_retry::{Action, RetryIf};

#[derive(Serialize, Deserialize)]
pub struct RetryConfig {
    pub retry_base_millis: u64,
    pub retry_max_delay_millis: u64,
    pub max_retries: usize,
}

pub struct Retry {
    strategy: Take<ExponentialBackoff>,
}

impl Retry {
    pub fn new(config: RetryConfig) -> Self {
        Retry {
            strategy: ExponentialBackoff::from_millis(config.retry_base_millis)
                .max_delay(Duration::from_millis(config.retry_max_delay_millis))
                .take(config.max_retries),
        }
    }

    fn log_condition<E, C>(err: &E, condition: &mut C) -> bool
    where
        E: Debug,
        C: FnMut(&E) -> bool,
    {
        if (*condition)(err) {
            debug!("Received error {:?}, retrying.", err);
            true
        } else {
            debug!("Received error {:?}, not retrying.", err);
            false
        }
    }

    pub async fn start<I, E, A>(&self, action: A) -> Result<I, E>
    where
        E: Debug,
        A: Action<Item = I, Error = E>,
    {
        self.start_with_condition(action, |_: &_| true).await
    }

    pub async fn start_with_condition<I, E, A, C>(
        &self,
        action: A,
        mut condition: C,
    ) -> Result<I, E>
    where
        E: Debug,
        A: Action<Item = I, Error = E>,
        C: FnMut(&E) -> bool,
    {
        let condition: Box<dyn FnMut(&E) -> bool> =
            Box::new(|err| Self::log_condition(err, &mut condition));
        RetryIf::spawn(self.strategy.clone(), action, condition).await
    }
}
