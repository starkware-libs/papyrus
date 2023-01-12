<div align="center">
  <h1>Papyrus</h1>
  <img src="./resources/img/papyrus-logo-square.png" height="200" width="200">
  <br />
  <a href="https://github.com/starkware-libs/papyrus/issues/new?assignees=&labels=bug&template=01_BUG_REPORT.md&title=bug%3A+">Report a Bug</a>
  ·
  <a href="https://github.com/starkware-libs/papyrus/issues/new?assignees=&labels=enhancement&template=02_FEATURE_REQUEST.md&title=feat%3A+">Request a Feature</a>
  ·
  <a href="https://github.com/starkware-libs/papyrus/discussions">Ask a Question</a>
</div>

<div align="center">
<br />

![GitHub Workflow Status](https://img.shields.io/github/actions/workflow/status/starkware-libs/papyrus/ci.yml?branch=main)
[![Project license](https://img.shields.io/github/license/starkware-libs/papyrus.svg?style=flat-square)](LICENSE)
[![Pull Requests welcome](https://img.shields.io/badge/PRs-welcome-ff69b4.svg?style=flat-square)](https://github.com/starkware-libs/papyrus/issues?q=is%3Aissue+is%3Aopen+label%3A%22help+wanted%22)

</div>

<details open="open">
<summary>Table of Contents</summary>

- [:warning: Disclaimer](#warning-disclaimer)
- [About](#about)
- [Getting Started](#getting-started)
  - [Compiling and running `papyrus`](#compiling-and-running-papyrus)
  - [Configuration](#configuration)
- [Running `papyrus` with Docker](#running-papyrus-with-docker)
- [Endpoints](#endpoints)
- [Roadmap](#roadmap)
- [Support](#support)
- [Project assistance](#project-assistance)
- [Contributing](#contributing)
- [Authors \& contributors](#authors--contributors)
- [Security](#security)
- [License](#license)

</details>

---

## :warning: Disclaimer

:warning: :construction: `Papyrus` is still being built therefore breaking changes might happen often so use it at your own risks.:construction: :warning:

## About

`Papyrus` is a StarkNet full node written in Rust.

## Getting Started

### Compiling and running `papyrus`

Prerequisites

- [Rust](https://www.rust-lang.org/tools/install)

You can build and run a `papyrus` node with the default configuration by running:

```bash
mkdir data
cargo run --release --package papyrus_node --bin papyrus_node
```

### Configuration

`Papyrus` supports configuration from command-line arguments and a configuration yaml file.
In case both are provided, the command-line arguments take precedence.
The default path for the configuration file is `config/config.yaml`. You can override this path
using the `--config_file` command-line argument.
See the default [configuration file](config/config.yaml) for available options.
Note that the configuration file can be partial or even empty.
You can check the available command-line arguments by running:

```bash
cargo run --release --package papyrus_node --bin papyrus_node -- --help
```

## Running `papyrus` with Docker

Prerequisites

- [Docker](https://docs.docker.com/get-docker/)

You can run a `papyrus` node with the default configuration by running:

```bash
docker run --rm --name papyrus\
  -p 8080-8081:8080-8081 \
  -v <local-host-data-path>:/app/data \
  ghcr.io/starkware-libs/papyrus:dev
```

Note:

- The container must have write access to `<local-host-data-path>`.
A possible way to assure this is to create the `<local-host-data-path>` directory (only the first
time you run `papyrus`) and add `--user "$(id -u):$(id -g)"` to the docker run command.
- You must include the `dev` tag which keeps track of our development branch and contains the most
up-to-date code. Once we have official releases we will add a `latest` tag for the latest release.
- Currently, there is no automatic upgrade mechanism.
Make sure to periodically pull the latest image and re-run the node.

## Endpoints

| Endpoint                                   | Supported          |
| :----------------------------------------- | :----------------- |
| `starknet_addDeclareTransaction`           | :x:                |
| `starknet_addDeployAccountTransaction`     | :x:                |
| `starknet_addInvokeTransaction`            | :x:                |
| `starknet_blockHashAndNumber`              | :heavy_check_mark: |
| `starknet_blockNumber`                     | :heavy_check_mark: |
| `starknet_call`                            | :x:                |
| `starknet_chainId`                         | :heavy_check_mark: |
| `starknet_estimateFee`                     | :x:                |
| `starknet_getBlockTransactionCount`        | :heavy_check_mark: |
| `starknet_getBlockWithTxHashes`            | :heavy_check_mark: |
| `starknet_getBlockWithTxs`                 | :heavy_check_mark: |
| `starknet_getClass`                        | :heavy_check_mark: |
| `starknet_getClassAt`                      | :heavy_check_mark: |
| `starknet_getClassHashAt`                  | :heavy_check_mark: |
| `starknet_getEvents`                       | :heavy_check_mark: |
| `starknet_getNonce`                        | :heavy_check_mark: |
| `starknet_getStateUpdate`                  | :heavy_check_mark: |
| `starknet_getStorageAt`                    | :heavy_check_mark: |
| `starknet_getTransactionByBlockIdAndIndex` | :heavy_check_mark: |
| `starknet_getTransactionByHash`            | :heavy_check_mark: |
| `starknet_getTransactionReceipt`           | :heavy_check_mark: |
| `starknet_pendingTransactions`             | :x:                |
| `starknet_syncing`                         | :x:                |

## Roadmap

See the [open issues](https://github.com/starkware-libs/papyrus/issues) for a list of proposed features (and known issues).

- [Top Feature Requests](https://github.com/starkware-libs/papyrus/issues?q=label%3Aenhancement+is%3Aopen+sort%3Areactions-%2B1-desc) (Add your votes using the 👍 reaction)
- [Top Bugs](https://github.com/starkware-libs/papyrus/issues?q=is%3Aissue+is%3Aopen+label%3Abug+sort%3Areactions-%2B1-desc) (Add your votes using the 👍 reaction)
- [Newest Bugs](https://github.com/starkware-libs/papyrus/issues?q=is%3Aopen+is%3Aissue+label%3Abug)

## Support

Reach out to the maintainer at one of the following places:

- [GitHub Discussions](https://github.com/starkware-libs/papyrus/discussions)
- Contact options listed on [this GitHub profile](https://github.com/starkware-libs)

## Project assistance

If you want to say **thank you** or/and support active development of Papyrus:

- Add a [GitHub Star](https://github.com/starkware-libs/papyrus) to the project.
- Tweet about the Papyrus.
- Write interesting articles about the project on [Dev.to](https://dev.to/), [Medium](https://medium.com/) or your personal blog.

Together, we can make Papyrus **better**!

## Contributing

First off, thanks for taking the time to contribute! Contributions are what make the open-source community such an amazing place to learn, inspire, and create. Any contributions you make will benefit everybody else and are **greatly appreciated**.

Please read [our contribution guidelines](docs/CONTRIBUTING.md), and thank you for being involved!

## Authors & contributors

For a full list of all authors and contributors, see [the contributors page](https://github.com/starkware-libs/papyrus/contributors).

## Security

Papyrus follows good practices of security, but 100% security cannot be assured.
Papyrus is provided **"as is"** without any **warranty**. Use at your own risk.

_For more information and to report security issues, please refer to our [security documentation](docs/SECURITY.md)._

## License

This project is licensed under the **Apache 2.0 license**.

See [LICENSE](LICENSE) for more information.
