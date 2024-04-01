# papyrus-config

## Description

papyrus-config is a flexible and powerful layered configuration system. This system allows you to easily create and manage configurations for your application.
The crate was designed specifically for Papyrus, a Starknet node, but can be used for any application.

## What you get
- Manage all things regarding your applications configuration in a single struct.
- Automatically generated configuration reference file and command (with full help section).
- Multiple sources for providing configuration. Command example: ```ARG1=1 application --config_file USER_PROVIDED_PATH.json --arg1=1 --arg2=2``` (the rest of the arguments will get the default value).
- Additional features (see below).

## How to use
1. Create a configuration struct and implement the `SerializeConfig` trait.
2. Call the traits method `dump_to_file`. This will create a reference file for the configuration. Save the file in a path available for your application. Note that users should not change this file.
3. In your application code, call `load_and_process_config` and pass it the default configuration file, a [Clap Command object](https://docs.rs/clap/latest/clap/) and the command args.
4. The `load_and_process_config` function will return an instance of your configuration struct filled with the user provided values.

For detailed information and examples, refer to the full [documentation](https://docs.rs/papyrus_config/)

## Configuration sources

Supports multiple configuration sources in ascending order of overriding priority:

- Default values
- Configuration files (from first to last)
- Environment variables
- Command-line arguments

## Additional features

- **Support for Nested Configuration Components:** Organize your configurations into nested components, making it easy to manage complex settings for different aspects of the application.

- **Usage of Pointers:** Use pointers to merge parameters that are common to multiple components. This capability helps in streamlining configurations and avoiding duplication of settings.

- **Automatically-Generated Command Line Parser:** To simplify the process of handling command-line arguments, the system automatically generates a command-line parser. This means you don't have to write complex argument parsing code; it's ready to use out-of-the-box.

- **Automatically-Generated Reference Configuration File:** Makes it easier for users by generating a reference configuration file. This file serves as a template that highlights all available configuration options and their default values, enabling users to customize their configurations efficiently.

## Documentation

Developer reference documentation is available at https://docs.rs/papyrus_config/. The documentation on this site is updated periodically.

To view the most up-to-date documentation, enter the following command at the root directory of the `papyrus` project:

```shell
cargo doc --open -p papyrus_config
```
