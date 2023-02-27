#!/usr/bin/env bash
set -euxo pipefail

BUILD_ID=$1
DURRATION=$2
DURRATION_TIMEOUT=$3
BASE_PATH="deployments/helm/CI"
TMP_FILE="$BASE_PATH/load_test_job-$BUILD_ID.yaml"

# copy the load-test tmeplate file and render it with supplied values
cp "$BASE_PATH/load_test_job.yaml.tmpl" "$TMP_FILE"
sed -i "s/BUILD_ID/$BUILD_ID/g" "$TMP_FILE"
sed -i "s/DURRATION/$DURRATION/g" "$TMP_FILE"

# create the load-test job
kubectl --namespace papyrus apply -f "$TMP_FILE" --wait=true

# wait for load-test pod to start
kubectl wait --namespace=papyrus --for=condition=ready pod -l job-name=papyrus-"$BUILD_ID"-load-test

# wait $DURRATION_TIMEOUT for the load-test application to finish
kubectl wait --namespace=papyrus --for=condition=complete job/papyrus-"$BUILD_ID"-load-test --timeout "$DURRATION_TIMEOUT"

# Save logs to file
kubectl logs --namespace=papyrus -l job-name=papyrus-"$BUILD_ID"-load-test --tail=-1 >"load_test_job-$BUILD_ID.out"

# delete the load-test job
kubectl --namespace papyrus delete -f "$TMP_FILE" --wait=true
rm "$TMP_FILE"
