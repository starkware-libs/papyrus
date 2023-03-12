# Contributing

When contributing to this repository, please first discuss the change you wish to make via issue,
email, or any other method with the owners of this repository before making a change.
Please note we have a [code of conduct](CODE_OF_CONDUCT.md),
please follow it in all your interactions with the project.

## Issues and feature requests

You've found a bug in the source code, a mistake in the documentation or maybe you'd like a new
feature? Take a look at [GitHub Discussions](https://github.com/starkware-libs/papyrus/discussions)
to see if it's already being discussed. You can help us by
[submitting an issue on GitHub](https://github.com/starkware-libs/papyrus/issues). Before you create
an issue, make sure to search the issue archive -- your issue may have already been addressed!

Please try to create bug reports that are:

- _Reproducible._ Include steps to reproduce the problem.
- _Specific._ Include as much detail as possible: which version, what environment, etc.
- _Unique._ Do not duplicate existing opened issues.
- _Scoped to a Single Bug._ One bug per report.

**Even better: Submit a pull request with a fix or new feature!**

## How to submit a Pull Request

1. Search our repository for open or closed
   [Pull Requests](https://github.com/starkware-libs/papyrus/pulls)
   that relate to your submission. You don't want to duplicate effort.
2. Fork the project
3. Create your feature branch (`git checkout -b feat/amazing_feature`)
4. Commit your changes (`git commit -m 'feat: add amazing_feature'`)
5. Push to the branch (`git push origin feat/amazing_feature`)
6. [Open a Pull Request](https://github.com/starkware-libs/papyrus/compare?expand=1)


## Development environment setup

In order to set up a development environment, First clone the repository:
```sh
git clone https://github.com/starkware-libs/papyrus
```

Then, you will need to install
- [Rust](https://www.rust-lang.org/tools/install) (1.67 or higher)
- [Rust nightly toolchain 2022-07-27](https://rust-lang.github.io/rustup/installation/index.html#installing-nightly)
- [Ganache 7.4.3](https://www.npmjs.com/package/ganache)
  - You'll need to install 7.4.3 and not a version above it. We'll relax this in the future.
  - You'll need Ganache only for the tests of the [papyrus_base_layer](../crates/papyrus_base_layer/) crate.

### CI
Your code will need to pass [CI](../.github/workflows/ci.yml) before it can be merged. This means your code will need to:
- Pass all local tests and all integration tests.
- Be formatted according to [rustfmt](https://github.com/rust-lang/rustfmt).
- Be linted according to [clippy](https://github.com/rust-lang/rust-clippy)
- Not include unused dependencies (Checked by [udeps](https://github.com/est31/cargo-udeps)).
