use std::sync::{Arc, Mutex};
use std::time::Instant;

use super::*;

struct Worker {
    count: Arc<Mutex<usize>>,
    total: Arc<usize>,
    time: Arc<Mutex<Instant>>,
    time_between: Arc<Mutex<Duration>>,
}

impl Worker {
    fn new(total: usize) -> Self {
        Worker {
            count: Arc::new(Mutex::new(0)),
            total: Arc::new(total),
            time: Arc::new(Mutex::new(Instant::now())),
            time_between: Arc::new(Mutex::new(Duration::ZERO)),
        }
    }

    fn get_last_delay(&self) -> u128 {
        self.time_between.lock().unwrap().as_millis()
    }

    fn get_last_attempt_count(&self) -> usize {
        *self.count.lock().unwrap()
    }

    async fn work(&self) -> Result<(), &str> {
        let mut time_between_guard = self.time_between.lock().unwrap();
        let mut time_guard = self.time.lock().unwrap();
        let mut count_guard = self.count.lock().unwrap();

        *time_between_guard = time_guard.elapsed();
        *time_guard = Instant::now();
        *count_guard += 1;

        if *count_guard < *self.total { Err("Some error.") } else { Ok(()) }
    }
}

#[tokio::test]
async fn test_fail_on_all_attempts() {
    let config = get_retry_test_config();
    let worker = Worker::new(10);
    Retry::new(&config).start(|| worker.work()).await.unwrap_err();
    assert!((40..42).contains(&worker.get_last_delay()));
    assert_eq!(worker.get_last_attempt_count(), 5);
}

#[tokio::test]
async fn test_success_on_third_attempt() {
    let config = get_retry_test_config();
    let worker = Worker::new(3);
    Retry::new(&config).start(|| worker.work()).await.unwrap();
    assert!((9..11).contains(&worker.get_last_delay()));
    assert_eq!(worker.get_last_attempt_count(), 3);
}
