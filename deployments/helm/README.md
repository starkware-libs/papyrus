# Papyrus helm chart installation

## Usage

```bash
helm upgrade --install `release_name` eployments/helm/ \
--namespace `namespace_name` --create-namespace \
--set ingress.host=`ingress_hostname` \
--set ethereumApiUrl=`ethereum_api_url`
```
