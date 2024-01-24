import argparse
import json
import subprocess
import sys

GRAFANA_TEMPLATE_FILE_PATH = "Monitoring/alerts_grafana.json"
GRAFANA_DESTINATION_FILE_PATH = "deployments/helm/Monitoring/alerts_grafana.json"


def parse_command_line_args():
    parser = argparse.ArgumentParser(description="Install Papyrus node.")
    parser.add_argument(
        "--pod_name", type=str, required=True, help="Name for the deployed pod."
    )
    parser.add_argument(
        "--namespace", type=str, required=True, help="Target namespace for the Papyrus node."
    )
    parser.add_argument(
        "--create_namespace",
        action="store_true",
        default=False,
        help="Enabling this option will install a new namespace with the given name.",
    )
    parser.add_argument(
        "--with_grafana",
        action="store_true",
        default=False,
        help="Enabling this option will also deploy a grafana deashboard with the pod.",
    )
    parser.add_argument(
        "--prometheus_uid",
        type=str,
        required=False,
        help="UID for prometheus (to use with Grafana)."
    )
    parser.add_argument(
        "--dry_run",
        action="store_true",
        default=False,
        help="Enabling this option will dry run the helm upgrade.",
    )

    return parser.parse_args()

def generate_grafana_tokens(namespace: str, prometheus_uid: str):
    grafana_template_lines = open(GRAFANA_TEMPLATE_FILE_PATH).readlines()
    grafana_dashboard_lines = list()
    for line in grafana_template_lines:
        grafana_dashboard_lines.append(
            line.replace("NAMESPACE", namespace).replace("${DS_PROMETHEUS}", prometheus_uid))
    grafana_dashboard = "".join(line for line in grafana_dashboard_lines)
    grafana_deployment_file = open(GRAFANA_DESTINATION_FILE_PATH, "r+")
    # Delete previous file contents.
    grafana_deployment_file.truncate(0)
    grafana_deployment_file.write(grafana_dashboard)
    grafana_deployment_file.flush()

def main():
    args = parse_command_line_args()
    print(args)
    cmd = f"helm upgrade --install {args.pod_name} deployments/helm/ --namespace {args.namespace}"
    if args.create_namespace:
        cmd += " --create-namespace"
    if args.with_grafana:
        assert args.prometheus_uid is not None, "Must provide Prometheus UID when deploying with Grafana."
        generate_grafana_tokens(namespace=args.namespace, prometheus_uid=args.prometheus_uid)
    if args.dry_run:
        cmd += " --dry-run"

    print(f"running {cmd}...")
    subprocess.Popen(cmd, shell=True)

if __name__ == "__main__":
    sys.exit(main())
