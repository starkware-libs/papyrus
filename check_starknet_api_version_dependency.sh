#!/bin/bash

set -e

# Fetch the package's dependencies using cargo metadata
CARGO_METADATA=$(cargo metadata --no-deps --format-version 1)

# Extract the names of the dependencies from the metadata using jq
DEPENDENCIES=$(echo "$CARGO_METADATA" | jq '.packages[].dependencies[].name')

# cleanup the dependencies names from duble quotes
DEPENDENCIES=$(echo "$DEPENDENCIES" | grep -oE "\"[^\"]+\"" | tr -d "\"")

if [[ $DEPENDENCIES =~ starknet_api ]]; then
    # Use jq to extract the dependencies which have the name "starknet_api" and get their source information.
    # The result will be the Git revision or other source details, if "starknet_api" is a dependency.
    SOURCE_INFO=$(echo "$CARGO_METADATA" | jq '.packages[].dependencies[] | select(.name == "starknet_api") | .source')

    # cleanup and sort.
    REVISION=$(echo "$SOURCE_INFO" | tr -d "\"" | sort -u)

    if [[ $REVISION =~ ^git\+https:\/\/github.com\/starkware-libs\/starknet-api\?rev=([a-f0-9]+)$ ]]; then
        # Extract the commit hash from the Git URL.
        COMMIT_HASH=${BASH_REMATCH[1]}

        # Check if the commit exists in the main branch of the "starknet-api" repository.
        GIT_API_URL="https://api.github.com/search/commits?q=repo:starkware-libs/starknet-api+hash:"
        HTTP_RESPONSE=$(curl -s -w "%{http_code}" "${GIT_API_URL}${COMMIT_HASH}" | tr -d '\n' | grep -o --regex='{.*}')
        TOTAL_COUNT=$(echo "$HTTP_RESPONSE" | jq '.total_count')

        if [[ "$TOTAL_COUNT" -eq 1 ]]; then
            echo "starknet_api is dependent on a git revision, and the commit is part of the main branch."
            exit 0
        else
            echo "starknet_api is dependent on a git revision, but the commit is not part of the main branch."
            exit 1
        fi
    else
        echo "starknet_api is dependent on a non-git version."
        exit 0
    fi
else
    echo "starknet_api is not a dependency."
    exit 0
fi
