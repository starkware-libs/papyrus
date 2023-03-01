# Papyrus helm chart installation

## Usage

```bash
helm upgrade --install `release_name` deployments/helm/ \
--namespace `namespace_name` --create-namespace \
--set ingress.host=`ingress_hostname`
```
