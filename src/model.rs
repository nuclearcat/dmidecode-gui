use dmidecode_rs::{Inventory, LoadOptions};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const CATEGORIES: &[&str] = &[
    "All records",
    "System",
    "BIOS",
    "Motherboard",
    "CPU",
    "Memory",
    "Slots & ports",
    "Other",
];
pub fn category(kind: u8) -> usize {
    match kind {
        1 | 3 | 12 | 15 | 23 | 32 => 1,
        0 | 13 => 2,
        2 | 10 | 41 => 3,
        4 | 7 => 4,
        5 | 6 | 16..=20 => 5,
        8 | 9 => 6,
        _ => 7,
    }
}

#[derive(Serialize, Deserialize)]
pub struct Record {
    pub handle: u16,
    pub kind: u8,
    pub title: String,
    pub fields: Vec<(String, String)>,
    pub json: String,
    pub raw: String,
    pub oem: Option<String>,
    pub search: String,
    pub memory: Option<String>,
    pub capacity_kib: Option<u64>,
    pub values: std::collections::BTreeMap<String, String>,
}
#[derive(Serialize, Deserialize)]
pub struct Snapshot {
    pub source: String,
    #[serde(default)]
    pub hostname: Option<String>,
    pub records: Vec<Record>,
    pub json: String,
}

pub fn flatten(prefix: &str, value: &Value, out: &mut Vec<(String, String)>) {
    match value {
        Value::Object(map) if !map.is_empty() => {
            for (key, value) in map {
                let key = key.replace('_', " ");
                let path = if prefix.is_empty() {
                    key
                } else {
                    format!("{prefix} / {key}")
                };
                flatten(&path, value, out);
            }
        }
        Value::Array(array) if !array.is_empty() => {
            for (i, value) in array.iter().enumerate() {
                flatten(&format!("{prefix} [{}]", i + 1), value, out);
            }
        }
        _ => out.push((
            prefix.into(),
            match value {
                Value::Null => "Not reported".into(),
                Value::String(s) => s.clone(),
                _ => value.to_string(),
            },
        )),
    }
}
impl Snapshot {
    pub fn live() -> Result<Self, String> {
        let mut snapshot =
            Self::decode(Inventory::load(&LoadOptions::default()).map_err(|e| e.to_string())?)?;
        snapshot.hostname = std::process::Command::new("hostname")
            .output()
            .ok()
            .filter(|output| output.status.success())
            .and_then(|output| String::from_utf8(output.stdout).ok())
            .map(|name| name.trim().to_owned())
            .filter(|name| !name.is_empty());
        Ok(snapshot)
    }
    pub fn export_filename(&self) -> String {
        let Some(hostname) = &self.hostname else {
            return "smbios.json".into();
        };
        let hostname: String = hostname
            .chars()
            .take(200)
            .map(|c| {
                if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        if hostname.is_empty() {
            "smbios.json".into()
        } else {
            format!("smbios-{hostname}.json")
        }
    }
    pub fn decode(inventory: Inventory) -> Result<Self, String> {
        if inventory.data.iter().next().is_none() {
            return Err("No SMBIOS records found in this input.".into());
        }
        let records = inventory
            .records()
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|r| {
                let device = inventory
                    .data
                    .find_by_handle(&dmidecode_rs::smbios::Handle(r.handle))
                    .and_then(|raw| raw.as_type::<dmidecode_rs::smbios::SMBiosMemoryDevice<'_>>());
                let capacity_kib = device
                    .as_ref()
                    .and_then(|d| memory_kib(d.size(), d.extended_size()));
                let memory =
                    device.map(|device| memory_label(device.size(), device.extended_size()));
                let title = match r.kind {
                    0 => "BIOS",
                    1 => "System",
                    2 => "Baseboard",
                    3 => "Chassis",
                    4 => "Processor",
                    7 => "Cache",
                    8 => "Port connector",
                    9 => "System slot",
                    16 => "Memory array",
                    17 => "Memory device",
                    19 => "Memory array mapping",
                    20 => "Memory device mapping",
                    127 => "End of table",
                    128..=255 => "OEM record",
                    _ => "SMBIOS record",
                }
                .to_string();
                let mut fields = Vec::new();
                flatten("", &r.fields, &mut fields);
                let oem = r.oem.map(|o| format!("{o:#?}"));
                let json = serde_json::to_string_pretty(&r.fields).unwrap_or_default();
                let mut raw = r
                    .raw
                    .chunks(16)
                    .enumerate()
                    .map(|(i, row)| {
                        format!(
                            "{:04X}  {}",
                            i * 16,
                            row.iter()
                                .map(|b| format!("{b:02X}"))
                                .collect::<Vec<_>>()
                                .join(" ")
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                for (i, s) in r.strings.iter().enumerate() {
                    raw.push_str(&format!("\nString {}: {s}", i + 1));
                }
                let mut values = crate::presentation::values(&r.fields);
                if let Some(label) = &memory {
                    values.insert("size".into(), label.clone());
                }
                let readable = values
                    .iter()
                    .map(|(k, v)| format!("{} {v}", crate::presentation::humanize(k)))
                    .collect::<Vec<_>>()
                    .join(" ");
                let search = format!(
                    "{title} {} 0x{:04x} {json} {readable} {}",
                    r.kind,
                    r.handle,
                    oem.as_deref().unwrap_or("")
                )
                .to_lowercase();
                Record {
                    handle: r.handle,
                    kind: r.kind,
                    title,
                    fields,
                    json,
                    raw,
                    oem,
                    search,
                    memory,
                    capacity_kib,
                    values,
                }
            })
            .collect();
        let json = inventory.to_json(true).map_err(|e| e.to_string())?;
        Ok(Self {
            source: inventory.source,
            hostname: None,
            records,
            json,
        })
    }
}

fn memory_kib(
    size: Option<dmidecode_rs::smbios::MemorySize>,
    extended: Option<dmidecode_rs::smbios::MemorySizeExtended>,
) -> Option<u64> {
    use dmidecode_rs::smbios::{MemorySize as S, MemorySizeExtended as E};
    match size {
        Some(S::NotInstalled) => Some(0),
        Some(S::Kilobytes(kb)) => Some(kb as u64),
        Some(S::Megabytes(mb)) => Some(mb as u64 * 1024),
        Some(S::SeeExtendedSize) => match extended {
            Some(E::Megabytes(mb)) => Some(mb as u64 * 1024),
            _ => None,
        },
        _ => None,
    }
}

fn memory_label(
    size: Option<dmidecode_rs::smbios::MemorySize>,
    extended: Option<dmidecode_rs::smbios::MemorySizeExtended>,
) -> String {
    use dmidecode_rs::smbios::{MemorySize as S, MemorySizeExtended as E};
    match size {
        Some(S::NotInstalled) => "Empty slot".into(),
        Some(S::Kilobytes(kb)) => crate::presentation::capacity(kb as u64),
        Some(S::Megabytes(mb)) => crate::presentation::capacity(mb as u64 * 1024),
        Some(S::SeeExtendedSize) => match extended {
            Some(E::Megabytes(mb)) => crate::presentation::capacity(mb as u64 * 1024),
            _ => "Capacity unknown".into(),
        },
        _ => "Capacity unknown".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn workstation_summary_preserves_identity_and_searches_display_units() {
        let s = Snapshot::decode(Inventory::from_bytes(
            include_bytes!("../tests/fixtures/workstation.bin").to_vec(),
            None,
        ))
        .unwrap();
        assert_eq!(
            s.records.iter().find(|r| r.kind == 1).unwrap().name(),
            "Engineering Workstation"
        );
        let modules: Vec<_> = s.records.iter().filter(|r| r.kind == 17).collect();
        assert_eq!(
            modules.iter().filter_map(|r| r.capacity_kib).sum::<u64>(),
            64 * 1024 * 1024
        );
        assert_eq!(
            modules
                .iter()
                .filter(|r| r.search.contains("32 gib"))
                .count(),
            2
        );
        assert_eq!(modules[1].value("memory_type"), "DDR5");
        assert_eq!(modules[1].value("configured_memory_speed"), "5200 MT/s");
        assert_eq!(modules[1].name(), "DIMM_A2");
    }
    #[test]
    fn server_demo_has_extended_thread_count_and_twelve_memory_channels() {
        let snapshot = Snapshot::decode(Inventory::from_bytes(
            include_bytes!("../tests/fixtures/server.bin").to_vec(),
            None,
        ))
        .unwrap();
        let cpu = snapshot.records.iter().find(|r| r.kind == 4).unwrap();
        assert_eq!(cpu.name(), "AMD EPYC 9754");
        assert_eq!(cpu.value("core_count"), "128");
        assert_eq!(cpu.value("thread_count"), "256");
        assert_eq!(cpu.value("processor_family"), "AMD Zen Processor Family");
        let modules: Vec<_> = snapshot.records.iter().filter(|r| r.kind == 17).collect();
        assert_eq!(modules.len(), 12);
        assert_eq!(
            modules.iter().filter_map(|r| r.capacity_kib).sum::<u64>(),
            1536 * 1024 * 1024
        );
        assert!(modules
            .iter()
            .all(|r| r.value("configured_memory_speed") == "4800 MT/s"));
        assert!(snapshot
            .records
            .iter()
            .find(|r| r.kind == 1)
            .unwrap()
            .name()
            .contains("Simulated"));
    }
    #[test]
    fn empty_input_is_rejected() {
        assert!(Snapshot::decode(Inventory::from_bytes(Vec::new(), None)).is_err());
    }
    #[test]
    fn memory_capacity_distinguishes_empty_unknown_and_extended() {
        use dmidecode_rs::smbios::{MemorySize as S, MemorySizeExtended as E};
        assert_eq!(memory_label(Some(S::NotInstalled), None), "Empty slot");
        assert_eq!(memory_label(None, None), "Capacity unknown");
        assert_eq!(memory_label(Some(S::Unknown), None), "Capacity unknown");
        assert_eq!(
            memory_label(Some(S::SeeExtendedSize), Some(E::Megabytes(65536))),
            "64 GiB"
        );
    }
    #[test]
    fn nested_fields_keep_arrays_nulls_and_false() {
        let mut rows = Vec::new();
        flatten(
            "",
            &serde_json::json!({"slots": [null, false, {"size_mb": 8192}]}),
            &mut rows,
        );
        assert!(rows.contains(&("slots [1]".into(), "Not reported".into())));
        assert!(rows.contains(&("slots [2]".into(), "false".into())));
        assert!(rows.contains(&("slots [3] / size mb".into(), "8192".into())));
    }
    #[test]
    fn raw_dump_becomes_searchable_snapshot() {
        let inv = Inventory::from_bytes(
            vec![
                1, 8, 0x34, 0x12, 1, 2, 0, 0, b'A', 0, b'B', 0, 0, 127, 4, 0xff, 0xff, 0, 0,
            ],
            None,
        );
        let snapshot = Snapshot::decode(inv).unwrap();
        assert_eq!(snapshot.records.len(), 2);
        assert_eq!(snapshot.records[0].handle, 0x1234);
        assert!(snapshot.records[0].search.contains("0x1234"));
        assert!(snapshot.records[0].fields.iter().any(|(_, v)| v == "B"));
        assert_eq!(category(17), 5);
    }
}
