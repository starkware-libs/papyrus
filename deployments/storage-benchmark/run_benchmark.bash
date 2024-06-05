#!/usr/bin/env bash
set -euxo pipefail

BASE_PATH="deployments/storage-benchmark"
PVC_FILE="$BASE_PATH/pvc.yaml"
DEPLOYMENT_FILE="$BASE_PATH/deployment.yaml"
CM_FILE="$BASE_PATH/cm.yaml"

NS="papyrus-storage-benchmark"
DURRATION_TIMEOUT=$1

# create a PVC with the benchmarked storage
kubectl --namespace "$NS" apply -f "$PVC_FILE" --wait=true

# create a configmap with the actions to run
kubectl --namespace "$NS" create configmap queries --from-file "$BASE_PATH/queries.txt" --dry-run=client --output yaml >"$CM_FILE"
kubectl --namespace "$NS" apply -f "$CM_FILE"

# create the storage-benchmark deployment
kubectl --namespace "$NS" apply -f "$DEPLOYMENT_FILE" --wait=true

# get the created pod name
POD=$(kubectl get pods -l app=storage-benchmark --namespace "$NS" --no-headers -o custom-columns=":metadata.name")

# wait for pod to start (since the benchmark is done in an initContainer, when the pod is Ready it
# means the benchmark is done).
kubectl wait --namespace "$NS" --for=condition=ready pod "$POD" --timeout "$DURRATION_TIMEOUT"

# get the results file
kubectl --namespace "$NS" cp --container results-export "$POD":/tmp/results/output.txt output.txt

# delete all temp resources from the cluster
kubectl --namespace "$NS" delete -f "$BASE_PATH/*.yaml" --wait=true
