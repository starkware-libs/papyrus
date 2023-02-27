#!/usr/bin/env bash
set -euxo pipefail

BUILD_ID=$1
DURRATION=$2
DURRATION_TIMEOUT=$3
BASE_PATH="deployments/helm/CI"
TMP_FILE="${BASE_PATH}/load_test_job-${BUILD_ID}.yaml"

cp "${BASE_PATH}/load_test_job.yaml.tmpl" "${TMP_FILE}"
sed -i "s/XXX/$BUILD_ID/g" "${TMP_FILE}"
sed -i "s/DURRATION/$DURRATION/g" "${TMP_FILE}"
kubectl --namespace papyrus apply -f "${TMP_FILE}" --wait=true

#wait for load-test pod to start
kubectl wait --namespace=papyrus --for=condition=ready pod -l job-name=papyrus-"$BUILD_ID"-load-test

# wait $DURRATION_TIMEOUT for the load-test application to finish
kubectl wait --namespace=papyrus --for=condition=complete job/papyrus-"${BUILD_ID}"-load-test --timeout "$DURRATION_TIMEOUT"

rm "${TMP_FILE}"
