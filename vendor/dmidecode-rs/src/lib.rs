//! SMBIOS loading and decoding for command-line and desktop applications.
//!
//! ```no_run
//! use dmidecode_rs::{Inventory, LoadOptions, smbios::SMBiosSystemInformation};
//! let inventory = Inventory::load(&LoadOptions::default())?;
//! if let Some(system) = inventory.data.first::<SMBiosSystemInformation<'_>>() {
//!     println!("{}", system.product_name());
//! }
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

pub mod oem;
#[cfg_attr(any(target_os = "linux", target_os = "freebsd"), path = "unix.rs")]
#[cfg_attr(windows, path = "windows.rs")]
#[cfg_attr(target_os = "macos", path = "macos.rs")]
mod platform;
/// Typed SMBIOS records and accessors, re-exported for library consumers.
pub use smbioslib as smbios;
use smbioslib::{SMBiosData, SMBiosVersion};
use std::{
    io,
    path::{Path, PathBuf},
};

/// Options for reading the local machine, independent of CLI argument parsing.
#[derive(Debug, Clone, Default)]
pub struct LoadOptions {
    /// Skip Linux sysfs and read physical memory instead.
    pub no_sysfs: bool,
    /// Override the physical-memory device path on Unix.
    pub dev_mem: Option<PathBuf>,
}

/// A decoded table and a description of its origin. Loading never prints output.
pub struct Inventory {
    /// Typed records, version, iteration and handle lookup.
    pub data: SMBiosData,
    /// Source and entry-point information suitable for display.
    pub source: String,
}

/// An owned record suitable for browsing without borrowing the original table.
#[derive(Debug)]
pub struct Record {
    pub handle: u16,
    pub kind: u8,
    pub fields: serde_json::Value,
    pub raw: Vec<u8>,
    pub strings: Vec<String>,
    pub oem: Option<oem::OemDecoded>,
}

impl Inventory {
    /// Read firmware from the local system. May require elevated access.
    pub fn load(options: &LoadOptions) -> io::Result<Self> {
        let (data, source) = platform::table_load(options)?;
        Ok(Self { data, source })
    }

    /// Load a saved SMBIOS dump using the same formats as the CLI.
    pub fn from_dump(path: impl AsRef<Path>) -> io::Result<Self> {
        let path = path.as_ref();
        let data = smbioslib::load_smbios_data_from_file(path)?;
        Ok(Self {
            data,
            source: format!("Getting SMBIOS data from {}.\n", path.display()),
        })
    }

    /// Decode raw SMBIOS table bytes (without an entry point).
    /// As with the underlying decoder, incomplete trailing records may be omitted.
    pub fn from_bytes(bytes: Vec<u8>, version: Option<SMBiosVersion>) -> Self {
        Self {
            data: SMBiosData::from_vec_and_version(bytes, version),
            source: "Raw SMBIOS table".into(),
        }
    }

    /// Serialize using the existing CLI JSON schema.
    pub fn to_json(&self, pretty: bool) -> serde_json::Result<String> {
        if pretty {
            serde_json::to_string_pretty(&self.data)
        } else {
            serde_json::to_string(&self.data)
        }
    }

    /// Decode every record, including the repository's vendor-aware OEM extensions.
    pub fn records(&self) -> serde_json::Result<Vec<Record>> {
        let context = oem::OemContext::from_data(&self.data);
        self.data
            .iter()
            .map(|record| {
                Ok(Record {
                    handle: *record.header.handle(),
                    kind: record.header.struct_type(),
                    fields: serde_json::to_value(record.defined_struct())?,
                    raw: record.fields.clone(),
                    strings: record
                        .strings
                        .iter()
                        .map(|s| String::from_utf8_lossy(s).into_owned())
                        .collect(),
                    oem: oem::decode_oem(record, &context),
                })
            })
            .collect()
    }
}
