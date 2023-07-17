#!/bin/bash

set -e

# Fetch the package's dependencies and check if "starknet_api" is one of them.
DEPENDENCIES=$(cargo metadata --no-deps --format-version 1 | jq '.packages[].dependencies[].name' | grep -oE "\"[^\"]+\"" | tr -d "\"")

if [[ $DEPENDENCIES =~ starknet_api ]]; then
    # If "starknet_api" is a dependency, fetch its Git revision.
    REVISION=$(cargo metadata --no-deps --format-version 1 | jq '.packages[].dependencies[] | select(.name == "starknet_api") | .source' | tr -d "\"" | sort -u)

    if [[ $REVISION =~ ^git\+https:\/\/github.com\/starkware-libs\/starknet-api\?rev=([a-f0-9]+)$ ]]; then
        # Extract the commit hash from the Git URL.
        COMMIT_HASH=${BASH_REMATCH[1]}
        
        # Check if the commit exists in the main branch of the "starknet-api" repository.
        GIT_API_URL="https://api.github.com/search/commits?q=repo:starkware-libs/starknet-api+hash:"
        HTTP_RESPONSE=$(curl -s -w "%{http_code}" "${GIT_API_URL}${COMMIT_HASH}" | tr -d '\n' | grep -o --regex='{.*}')
        echo $HTTP_RESPONSE
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
