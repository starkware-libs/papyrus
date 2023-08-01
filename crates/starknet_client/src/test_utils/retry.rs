use crate::retry::RetryConfig;

pub const MAX_RETRIES: usize = 4;

pub fn get_test_config() -> RetryConfig {
    RetryConfig { retry_base_millis: 3, retry_max_delay_millis: 40, max_retries: 4 }
}
