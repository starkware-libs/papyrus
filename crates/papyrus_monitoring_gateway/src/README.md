# Papyrus Monitoring API

## Papyrus Metrics

### Gateway Request Counters Metrics
A requests counter metric is produced for each of the Papyrus gateway endpoints.
Note: Separate counters are produced for different versions of the same endpoint.

Metric name: gateway_incoming_requests

### Gateway Failed Request Counters Metrics
A requests failures counter metric is produced for each of the Papyrus gateway endpoints.
Note: Separate counters are produced for different versions of the same endpoint.

Metric name: gateway_failed_requests

### Gateway Request Latency Metrics
Requests latency statistics metrics are produced for each of the Papyrus gateway endpoints.
Note: Separate counters are produced for different versions of the same endpoint.

Metrics list:
- gateway_request_latency_seconds_sum
- gateway_request_latency_seconds_count
- gateway_request_latency_seconds (% in quantiles)

### Block Markers
- Block header marker \
  Metric name: papyrus_header_marker
- Block body marker \
  Metric name: papyrus_body_marker
- Block state-diff marker \
  Metric name: papyrus_state_marker
- Block compiled-class marker \
  Metric name: papyrus_compiled_class_marker
- Central block marker \
  Metric name: papyrus_central_block_marker

### Miscellaneous Metrics

- Papyrus header latency gauge \
  Metric name: papyrus_header_latency
- Process resident memory bytes gauge \
  Metric name: process_resident_memory_bytes
- Process start time (seconds) gauge \
  Metric name: process_start_time_seconds
- Process virtual memory bytes gauge \
  Metric name: process_virtual_memory_bytes
- Process virtual memory max bytes gauge \
  Metric name: process_virtual_memory_max_bytes
- Process cpu total (seconds) gauge \
  Metric name: process_cpu_seconds_total
- Process threads gauge \
  Metric name: process_threads
- Process open fds gauge \
  Metric name: process_open_fds
- Process max fds gauge \
  Metric name: process_max_fds
