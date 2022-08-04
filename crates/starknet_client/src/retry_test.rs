use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use log::debug;

use super::Retry;
use crate::test_utils::retry::get_test_config;

struct Worker {
    // Number of times the worker was called. Updated in every call to work.
    number_of_calls: Arc<Mutex<usize>>,
    // Number of times the worker returns errors before it returns ok.
    number_of_errors: Arc<usize>,
    // The current time. Updated in every call to work.
    time: Arc<Mutex<Instant>>,
    // The time elapsed between calls to the worker. Updated in every call to work.
    time_between_calls: Arc<Mutex<Duration>>,
}

impl Worker {
    fn new(number_of_errors: usize) -> Self {
        Worker {
            number_of_calls: Arc::new(Mutex::new(0)),
            number_of_errors: Arc::new(number_of_errors),
            time: Arc::new(Mutex::new(Instant::now())),
            time_between_calls: Arc::new(Mutex::new(Duration::ZERO)),
        }
    }

    fn get_last_delay(&self) -> u128 {
        self.time_between_calls.lock().unwrap().as_millis()
    }

    fn get_last_attempt(&self) -> usize {
        *self.number_of_calls.lock().unwrap()
    }

    async fn work(&self) -> Result<(), &str> {
        let mut time_between_calls = self.time_between_calls.lock().unwrap();
        let mut time = self.time.lock().unwrap();
        let mut number_of_calls = self.number_of_calls.lock().unwrap();

        *time_between_calls = time.elapsed();
        *time = Instant::now();
        *number_of_calls += 1;

        if *number_of_calls <= *self.number_of_errors { Err("Some error.") } else { Ok(()) }
    }
}

#[tokio::test]
async fn test_fail_on_all_attempts() {
    let config = get_test_config();
    let worker = Worker::new(10);
    Retry::new(&config).start(|| worker.work()).await.unwrap_err();
    debug!("Worker last delay: {:?}", &worker.get_last_delay());
    assert!((40..43).contains(&worker.get_last_delay()));
    assert_eq!(worker.get_last_attempt(), 5);
}

#[tokio::test]
async fn test_success_on_third_attempt() {
    let config = get_test_config();
    let worker = Worker::new(2);
    Retry::new(&config).start(|| worker.work()).await.unwrap();
    debug!("Worker last delay: {:?}", &worker.get_last_delay());
    assert!((9..12).contains(&worker.get_last_delay()));
    assert_eq!(worker.get_last_attempt(), 3);
}
