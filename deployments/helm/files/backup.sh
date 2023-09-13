#!/usr/bin/env sh
set -x
if [ -z "${ADDITIONAL_HEADER}" ]; then
    ADDITIONAL_ARGS=""
else
    ADDITIONAL_ARGS="--http_headers=${ADDITIONAL_HEADER}"
fi

if [ -n "${CONCURRENT_REQUESTS}" ]; then
    # temporary workaround for an internal papyrus memory issue
    sed -i "s/concurrent_requests: 10/concurrent_requests: $CONCURRENT_REQUESTS/g" /app/config/config.yaml
fi

RUN_CMD="/app/target/release/papyrus_node --config_file=/app/config/presets/${PRESET} ${ADDITIONAL_ARGS}"

while true; do
    # start papyrus and save the pid
    sh -c "$RUN_CMD" &
    PAPYRUS_PID="$!"

    sleep "$SLEEP_INTERVAL"

    # stop papyrus
    kill -15 "$PAPYRUS_PID"
    sleep 5s

    TS=$(date +%s)
    if [ "$COMPRESS_BACKUP" = true ]; then
        # compress file, upload compressed file and delete the compressed file
        cd "/app/data/$CHAIN_ID" || exit 1
        TAR_FILE_NAME="$TS.tar.gz"
        tar -czvf "$TAR_FILE_NAME" mdbx.dat
        aws s3 cp "$TAR_FILE_NAME" "s3://$S3_BUCKET_NAME/$CHAIN_ID/$PAPYRUS_VERSION/$TAR_FILE_NAME"
        rm "$TAR_FILE_NAME"
        cd /app || exit 1
    else
        # upload db file to s3
        aws s3 cp "/app/data/$CHAIN_ID/mdbx.dat" "s3://$S3_BUCKET_NAME/$CHAIN_ID/$PAPYRUS_VERSION/$TS.dat"
    fi
done
