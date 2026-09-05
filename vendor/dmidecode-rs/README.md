# Vendored dmidecode-rs library

Source: https://github.com/nuclearcat/dmidecode-rs
Base commit: a2809dd7d540bcc91c18e85a3b2b79c349331a6d

This snapshot includes the local library refactor built for DMI Explorer:
`Inventory`, `LoadOptions`, owned decoded records, typed SMBIOS access, JSON,
and vendor-aware OEM decoding. The platform loaders use `LoadOptions` instead
of CLI arguments. The source files match that local library refactor.

Only the library sources, license, and hardware-independent library tests are
included. The manifest omits the CLI and its dependencies. The test comparing
CLI output is omitted because this snapshot does not contain a CLI binary.
The upstream MIT license is preserved in LICENSE.

The library is vendored so a fresh GUI checkout builds without a sibling
repository or unpublished changes. When updating it, keep the source files
in sync and run `cargo test --workspace` from the GUI repository root.
