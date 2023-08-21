# Papyrus Monitoring API

## Alive
Use the *alive* API call to check liveness of the Papyrus node. \
Usage: \
**curl https://<papyrus_url>/monitoring/alive**

## Papyrus Node Version
nodeVersion API call retrieves the Papyrus node version (GIT tag). \
Usage: \
**curl https://<papyrus_url>/monitoring/nodeVersion**

## Papyrus Node Config
nodeConfig API call retrieves the Papyrus node configuration. \
Usage: \
**curl https://<papyrus_url>/monitoring/nodeConfig**

## Papyrus Database Statistics
dbTablesStats API call retrieves statistics of the Papyrus node database. \
Usage: \
**curl https://<papyrus_url>/monitoring/dbTablesStats**

## Papyrus Metrics
To retrieve the Papyrus metrics, use the monitoring gateway metrics API. \
Usage: \
**curl https://<papyrus_url>/monitoring/metrics**

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
