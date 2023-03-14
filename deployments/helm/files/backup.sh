#!/usr/bin/env sh
set -x
if [ -z "${ADDITIONAL_HEADER}" ]; then
    ADDITIONAL_ARGS=""
else
    ADDITIONAL_ARGS="--http_headers=${ADDITIONAL_HEADER}"
fi
RUN_CMD="/app/target/release/papyrus_node --chain_id=${CHAIN_ID} --central_url=${CENTRAL_URL} ${ADDITIONAL_ARGS}"

while true; do
    # start papyrus and save the pid
    sh -c "$RUN_CMD" &
    PAPYRUS_PID="$!"

    sleep "$SLEEP_INTERVAL"

    # stop papyrus
    kill -15 "$PAPYRUS_PID"
    sleep 5s

    # upload db file to s3
    aws s3 cp "/app/data/$CHAIN_ID/mdbx.dat" "s3://$S3_BUCKET_NAME/$CHAIN_ID/$PAPYRUS_VERSION/$(date +%s).dat"
done
