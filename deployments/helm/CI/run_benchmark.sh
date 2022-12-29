#!/usr/bin/env sh
set -x

BUILD_ID=$1
BASE_PATH="deployments/helm/CI"
TMP_FILE="${BASE_PATH}/load_test_job-${BUILD_ID}.yaml"

cp "${BASE_PATH}/load_test_job.yaml.tmpl" "${TMP_FILE}"
sed -i "s/XXX/$BUILD_ID/g" "${TMP_FILE}"
kubectl --namespace papyrus apply -f "${TMP_FILE}" --wait

rm "${TMP_FILE}"
