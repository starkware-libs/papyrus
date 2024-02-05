import argparse
import json
import subprocess
import sys

GRAFANA_TEMPLATE_FILE_PATH = "Monitoring/alerts_grafana.json"
GRAFANA_DESTINATION_FILE_PATH = "deployments/helm/Monitoring/alerts_grafana.json"

# TODO: Add function to deploy monitoring dashboard.
def parse_command_line_args():
    parser = argparse.ArgumentParser(description="Install Papyrus node.")
    parser.add_argument(
        "--release_name", type=str, required=True, help="Name for the helm release."
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
        "--with_alerts",
        action="store_true",
        default=False,
        help="Enabling this option will also deploy a grafana alerts deashboard with the pod.",
    )
    parser.add_argument(
        "--prometheus_uid",
        type=str,
        required=False,
        help="UID for prometheus (to use with Grafana)."
    )
    parser.add_argument(
        "--old_version",
        type=str,
        required=False,
        help="Represents previous RPC version for the desired env (e.g. v0_3)."
    )
    parser.add_argument(
        "--new_version",
        type=str,
        required=False,
        help="Represents current RPC version for the desired env (e.g. v0_4)."
    )
    parser.add_argument(
        "--dry_run",
        action="store_true",
        default=False,
        help="Enabling this option will dry run the helm upgrade.",
    )
    parser.add_argument(
        "--helm_deployment_dir",
        type=str,
        required=False,
        default="./deployments/helm/",
        help="Relative path to the helm deployment directory (default is ./deployments/helm/."
    )

    return parser.parse_args()

def generate_grafana_tokens(namespace: str, prometheus_uid: str):
    grafana_template_lines = open(GRAFANA_TEMPLATE_FILE_PATH).readlines()
    grafana_dashboard_lines = list()
    for line in grafana_template_lines:
        grafana_dashboard_lines.append(
            line.replace("NAMESPACE", namespace).replace("${DS_PROMETHEUS}", prometheus_uid))
    grafana_dashboard = "".join(line for line in grafana_dashboard_lines)
    grafana_deployment_file = open(GRAFANA_DESTINATION_FILE_PATH, "a")
    # Delete previous file contents.
    grafana_deployment_file.truncate(0)
    grafana_deployment_file.write(grafana_dashboard)
    grafana_deployment_file.flush()

def main():
    args = parse_command_line_args()
    print(args)
    # The CMD assumes this script is being run from the root directory.
    cmd = f"helm upgrade --install {args.release_name} {args.helm_deployment_dir} --namespace {args.namespace}"
    if args.create_namespace:
        cmd += " --create-namespace"
    if args.with_alerts:
        assert args.prometheus_uid is not None, "Must provide Prometheus UID when deploying with Grafana."
        generate_grafana_tokens(namespace=args.namespace, prometheus_uid=args.prometheus_uid)
    if args.dry_run:
        cmd += " --dry-run"

    print(f"running {cmd}...")
    subprocess.Popen(cmd, shell=True)

if __name__ == "__main__":
    sys.exit(main())
