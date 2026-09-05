# DMI Explorer

A native Rust desktop viewer for SMBIOS hardware inventory, built with
egui/eframe and the dmidecode-rs library. No Python, webview, or CLI-output parsing.

![Overview using synthetic workstation data](docs/overview.png)

## Build and run

```sh
git clone git@github.com:nuclearcat/dmidecode-gui.git
cd dmidecode-gui
cargo run
# Preview with synthetic data; no firmware access required:
cargo run -- tests/fixtures/workstation.bin
# Open a saved SMBIOS binary dump:
cargo run -- /path/to/smbios.bin
# Optimized standalone executable:
cargo build --release
./target/release/dmidecode-gui
```

Built and tested with Rust 1.97.1. Requires a desktop graphics session. On Linux,
Wayland and X11 are supported through OpenGL. File dialogs use the desktop portal
(`xdg-desktop-portal` and your desktop's portal backend). A C linker and platform
development libraries are required to build eframe.

## Explore hardware

- Overview of system identity, processors, installed memory, motherboard, and firmware.
- Memory-slot cards distinguish installed capacity, empty slots, and unknown
  capacity. Unknown modules are excluded from the explicitly labeled known total.
- Component pages group specifications and normalize units and enum names.
  Firmware strings and identifiers are preserved.
- All records provides a list and detail pane on wider windows and a selector
  on narrower ones. Complete JSON, bytes, and OEM decoding live under Raw record.
- Search matches raw and displayed values. Ctrl+F focuses search; Escape clears
  it. Right-click a value to copy, or copy a complete record summary.
- Export saves the full table in the existing dmidecode-rs CLI JSON schema.
  OEM extensions are displayed separately and do not change that schema.

## Firmware access

Firmware reading runs on a worker thread. On Linux, **Read system → Read with
administrator access…** uses `pkexec` to run a separate reader process. The GUI
continues running as your normal user. This requires polkit and a desktop
authentication agent; no policy is installed automatically.

Saved SMBIOS binary dumps can be opened without administrator access, provided
your user can read the file. The GUI links directly to the vendored decoder;
it does not require a dmidecode-rs CLI executable or a sibling checkout.

## Development and verification

```sh
cargo test --workspace
cargo clippy --no-deps -- -D warnings
```

Tests cover library loading and OEM decoding, readable units, memory capacity,
search, selection, error handling, and rendering at supported window widths.
The test fixture contains synthetic data, not a real machine's inventory.

The optional screenshot feature captures only the app window after rendering
settles and closes the app after saving:

```sh
cargo build --features screenshots
DMI_SCREENSHOT_TO=/tmp/dmi-overview.png ./target/debug/dmidecode-gui tests/fixtures/workstation.bin
```

Use `DMI_SCREENSHOT_CATEGORY=0` through `7` for a component/inspector page and
`DMI_SCREENSHOT_WIDTH=900` for the minimum width. These switches only apply to
builds with the screenshot feature.

## Licensing and dependencies

The GUI is MIT licensed. The dmidecode-rs library source is included under
`vendor/dmidecode-rs`, with its upstream license and provenance. It includes the
library refactor needed by this GUI; see the vendored README for details.

Noto Sans is bundled for consistent readable text. Its SIL Open Font License
is included in `assets/FONT-LICENSE.txt`.
