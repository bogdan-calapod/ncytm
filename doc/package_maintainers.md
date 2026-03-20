# Packaging

> **Note**: ncytm is a fork of [ncspot](https://github.com/hrkfdn/ncspot). Packaging status shown below is for the original ncspot project.

[![Packaging status](https://repology.org/badge/vertical-allrepos/ncspot.svg)](https://repology.org/project/ncspot/versions)

## Compilation Instructions
ncytm makes use of the standard Cargo build system for everything. To compile a release version,
execute `cargo build --release` in the terminal from the project root. The executable file can be
found at `target/release/ncytm`. For detailed build instructions, have a look at [the developer
documentation](/doc/developers.md).

Additional features can be included by appending them to the build command. A list of all the
available features can be found in the [Cargo.toml](/Cargo.toml) under the `[features]` table. To
activate a feature, include its name like `cargo build --release --features feature1,feature2,...`.
To disable the default features, add `--no-default-features` to the command.

## Other Provided Files
The following is a list of other files that are provided by ncytm. Some of them need to be
generated. Execute `cargo xtask --help` for more information.
- LICENSE
- images/logo.svg (optional)
- misc/ncytm.desktop (for Linux systems)
- misc/*.1 (for Linux systems)
- misc/ncytm.bash (bash completions)
- misc/\_ncytm (zsh completions)
- misc/ncytm.fish (fish completions)
- misc/ncytm.elv (elvish completions)
- misc/\_ncytm.ps1 (powershell completions)

## Building a Debian Package
The [`cargo-deb`](https://github.com/kornelski/cargo-deb#readme) package can be used to build a
Debian package with the following commands. The package will be generated in `target/debian/`.

```sh
cargo install cargo-deb
cargo deb
```
