//! Human-facing hardware facts, separate from the lossless record inspector.
use crate::model::Record;
use serde_json::Value;
use std::collections::BTreeMap;

pub fn values(json: &Value) -> BTreeMap<String, String> {
    let body = json
        .as_object()
        .and_then(|o| o.values().next())
        .and_then(Value::as_object);
    let mut result: BTreeMap<String, String> = body
        .into_iter()
        .flat_map(|o| o.iter())
        .filter(|(key, _)| key.as_str() != "header")
        .filter_map(|(key, val)| display_value(val).map(|v| (key.clone(), with_unit(key, v))))
        .collect();
    for (base, extended) in [
        ("core_count", "core_count_2"),
        ("cores_enabled", "cores_enabled_2"),
        ("thread_count", "thread_count_2"),
        ("processor_family", "processor_family_2"),
        ("speed", "extended_speed"),
        (
            "configured_memory_speed",
            "extended_configured_memory_speed",
        ),
    ] {
        if !result.contains_key(base) {
            if let Some(value) = result.get(extended).cloned() {
                result.insert(base.into(), value);
            }
        }
    }
    result
}
fn with_unit(key: &str, value: String) -> String {
    if value.parse::<u64>().is_ok() {
        match key {
            "current_speed" | "max_speed" | "external_clock" => return format!("{value} MHz"),
            "minimum_voltage" | "maximum_voltage" | "configured_voltage" => {
                return format!("{value} mV")
            }
            _ => (),
        }
    }
    value
}
pub fn display_value(value: &Value) -> Option<String> {
    match value {
        Value::Null => None,
        Value::String(s)
            if s.trim().is_empty()
                || s.starts_with("The structure's field")
                || s.starts_with("The given string number") =>
        {
            None
        }
        Value::String(s) => Some(match s.as_str() {
            "NotInstalled" => "Not installed".into(),
            "SeeExtendedSize"
            | "SeeExtendedSpeed"
            | "SeeCoreCount2"
            | "SeeCoresEnabled2"
            | "SeeThreadCount2"
            | "SeeProcessorFamily2" => return None,
            "Unknown" | "None" => "Not reported".into(),
            _ => s.trim().to_owned(),
        }),
        Value::Bool(v) => Some(if *v { "Yes" } else { "No" }.into()),
        Value::Number(n) => Some(n.to_string()),
        Value::Object(map) => {
            if let Some(v) = map.get("value") {
                return display_value(v).map(|s| humanize(&s));
            }
            if map.len() == 1 {
                let (unit, v) = map.iter().next()?;
                let v = display_value(v)?;
                return Some(match unit.as_str() {
                    "Megabytes" => format!("{v} MiB"),
                    "Kilobytes" => format!("{v} KiB"),
                    "MTs" => format!("{v} MT/s"),
                    "MHz" => format!("{v} MHz"),
                    "Count" | "Cores" | "Threads" | "Uuid" => v,
                    _ => format!("{v} {}", humanize(unit)),
                });
            }
            let flags: Vec<_> = map
                .iter()
                .filter(|(k, v)| k.as_str() != "raw" && **v == Value::Bool(true))
                .map(|(k, _)| humanize(k))
                .collect();
            if flags.is_empty() {
                None
            } else {
                Some(flags.join(", "))
            }
        }
        Value::Array(a) => {
            let entries: Vec<_> = a.iter().filter_map(display_value).collect();
            if entries.is_empty() {
                None
            } else {
                Some(entries.join(", "))
            }
        }
    }
}
pub fn humanize(s: &str) -> String {
    // SMBIOS identifiers include acronyms and compressed units that cannot be
    // recovered by splitting underscores or camel case. Keep explicit labels.
    match s {
        "bios_characteristics_not_supported" => return "BIOS characteristics not supported".into(),
        "bios_upgradeable" => return "BIOS upgradeable".into(),
        "bios_shadowing_allowed" => return "BIOS shadowing allowed".into(),
        "bios_rom_socketed" => return "BIOS ROM socketed".into(),
        "vlvesa_supported" => return "VESA local bus supported".into(),
        "escd_support_available" => return "ESCD support available".into(),
        "boot_from_cdsupported" => return "Boot from CD supported".into(),
        "boot_from_pcmcia_supported" => return "Boot from PCMCIA supported".into(),
        "edd_specification_supported" => return "EDD specification supported".into(),
        "floppy_nec_japanese_supported" => {
            return "NEC 9800 Japanese 3.5-inch 1.2 MB floppy services supported".into()
        }
        "floppy_toshiba_japanese_supported" => {
            return "Toshiba Japanese 3.5-inch 1.2 MB floppy services supported".into()
        }
        "floppy_525_360_supported" => return "5.25-inch 360 KB floppy services supported".into(),
        "floppy_525_12_supported" => return "5.25-inch 1.2 MB floppy services supported".into(),
        "floppy_35_720_supported" => return "3.5-inch 720 KB floppy services supported".into(),
        "floppy_35_288_supported" => return "3.5-inch 2.88 MB floppy services supported".into(),
        "keyboard_8042services_supported" => return "8042 keyboard services supported".into(),
        "cga_mono_video_services_supported" => {
            return "CGA/monochrome video services supported".into()
        }
        "nec_pc_98supported" => return "NEC PC-98 supported".into(),
        "acpi_is_supported" => return "ACPI supported".into(),
        "usb_legacy_is_supported" => return "Legacy USB supported".into(),
        "agp_is_supported" => return "AGP supported".into(),
        "i2oboot_is_supported" => return "I2O boot supported".into(),
        "ls120super_disk_boot_is_supported" => return "LS-120 SuperDisk boot supported".into(),
        "atapi_zip_drive_boot_is_supported" => return "ATAPI ZIP drive boot supported".into(),
        "boot_1394is_supported" => return "IEEE 1394 boot supported".into(),
        "smart_battery_is_supported" => return "Smart battery supported".into(),
        "bios_boot_specification_is_supported" => {
            return "BIOS Boot Specification supported".into()
        }
        "fkey_initiated_network_boot_is_supported" => {
            return "Function-key-initiated network boot supported".into()
        }
        "smbios_table_describes_avirtual_machine" => {
            return "SMBIOS table describes a virtual machine".into()
        }
        "uefi_specification_is_supported" => return "UEFI specification supported".into(),
        "isa_supported" => return "ISA supported".into(),
        "mca_supported" => return "MCA supported".into(),
        "eisa_supported" => return "EISA supported".into(),
        "pci_supported" => return "PCI supported".into(),
        "pcmcia_supported" => return "PCMCIA supported".into(),
        "apm_supported" => return "APM supported".into(),

        "bit_64capable" => return "64-bit capable".into(),
        "Ddr" => return "DDR".into(),
        "Ddr2" => return "DDR2".into(),
        "Ddr3" => return "DDR3".into(),
        "Ddr4" => return "DDR4".into(),
        "Ddr5" => return "DDR5".into(),
        "Dimm" => return "DIMM".into(),
        "Sodimm" => return "SO-DIMM".into(),
        "uuid" => return "UUID".into(),
        "sku_number" => return "SKU number".into(),
        _ => (),
    }
    // Leave firmware strings and model identifiers intact; only split enum names.
    if s.contains(' ') || s.contains('-') || s.contains('.') {
        return s.to_owned();
    }
    let chars: Vec<_> = s.chars().collect();
    let mut out = String::new();
    for (i, &c) in chars.iter().enumerate() {
        if c == '_' {
            out.push(' ');
            continue;
        }
        if i > 0
            && c.is_uppercase()
            && (chars[i - 1].is_lowercase()
                || (chars[i - 1].is_uppercase()
                    && chars.get(i + 1).is_some_and(|c| c.is_lowercase())))
        {
            out.push(' ');
        }
        out.push(c);
    }
    if let Some(first) = out.get_mut(..1) {
        first.make_ascii_uppercase();
    }
    out
}

impl Record {
    pub fn value(&self, key: &str) -> &str {
        self.values
            .get(key)
            .map(String::as_str)
            .unwrap_or("Not reported")
    }
    pub fn name(&self) -> &str {
        let key = match self.kind {
            1 => "product_name",
            2 => "product",
            4 => "processor_version",
            17 => "device_locator",
            7 => "socket_designation",
            8 => "internal_reference_designator",
            9 => "slot_designation",
            _ => "",
        };
        self.values
            .get(key)
            .filter(|s| s.as_str() != "Not reported")
            .map(String::as_str)
            .unwrap_or(&self.title)
    }
    pub fn subtitle(&self) -> String {
        match self.kind {
            0 => format!("{} · {}", self.value("vendor"), self.value("version")),
            4 => format!(
                "{} · {}",
                self.value("socket_designation"),
                self.value("current_speed")
            ),
            17 => format!(
                "{} · {}",
                self.memory.as_deref().unwrap_or("Capacity unknown"),
                self.value("memory_type")
            ),
            _ => self
                .values
                .get("manufacturer")
                .cloned()
                .unwrap_or_else(|| self.title.clone()),
        }
    }
    pub fn groups(&self) -> Vec<(&'static str, Vec<(&'static str, String)>)> {
        let definitions: Vec<(&str, &[(&str, &str)])> = match self.kind {
            0 => vec![
                (
                    "Firmware",
                    &[
                        ("Vendor", "vendor"),
                        ("Version", "version"),
                        ("Release date", "release_date"),
                        ("ROM capacity", "rom_size"),
                    ],
                ),
                (
                    "Capabilities",
                    &[
                        ("BIOS features", "characteristics"),
                        ("Platform features", "characteristics_extension0"),
                        ("Boot features", "characteristics_extension1"),
                    ],
                ),
            ],
            1 => vec![
                (
                    "System identity",
                    &[
                        ("Manufacturer", "manufacturer"),
                        ("Product", "product_name"),
                        ("Family", "family"),
                        ("Version", "version"),
                        ("SKU", "sku_number"),
                    ],
                ),
                (
                    "Identifiers",
                    &[("Serial number", "serial_number"), ("UUID", "uuid")],
                ),
            ],
            2 => vec![
                (
                    "Board",
                    &[
                        ("Manufacturer", "manufacturer"),
                        ("Product", "product"),
                        ("Version", "version"),
                        ("Board type", "board_type"),
                    ],
                ),
                (
                    "Identifiers",
                    &[
                        ("Serial number", "serial_number"),
                        ("Asset tag", "asset_tag"),
                        ("Location", "location_in_chassis"),
                    ],
                ),
            ],
            4 => vec![
                (
                    "Processor",
                    &[
                        ("Manufacturer", "processor_manufacturer"),
                        ("Model", "processor_version"),
                        ("Socket", "socket_designation"),
                        ("Family", "processor_family"),
                    ],
                ),
                (
                    "Configuration",
                    &[
                        ("Cores", "core_count"),
                        ("Enabled cores", "cores_enabled"),
                        ("Threads", "thread_count"),
                        ("Reported speed", "current_speed"),
                        ("Maximum speed", "max_speed"),
                        ("Capabilities", "processor_characteristics"),
                    ],
                ),
            ],
            17 => vec![
                (
                    "Module",
                    &[
                        ("Slot", "device_locator"),
                        ("Bank", "bank_locator"),
                        ("Type", "memory_type"),
                        ("Form factor", "form_factor"),
                        ("Configured speed", "configured_memory_speed"),
                        ("Rated speed", "speed"),
                    ],
                ),
                (
                    "Manufacturer & identity",
                    &[
                        ("Manufacturer", "manufacturer"),
                        ("Part number", "part_number"),
                        ("Serial number", "serial_number"),
                        ("Configured voltage", "configured_voltage"),
                    ],
                ),
            ],
            9 => vec![(
                "Expansion slot",
                &[
                    ("Designation", "slot_designation"),
                    ("Type", "slot_type"),
                    ("Usage", "current_usage"),
                    ("Width", "slot_data_bus_width"),
                    ("Length", "slot_length"),
                ],
            )],
            _ => vec![],
        };
        definitions
            .into_iter()
            .map(|(name, keys)| {
                (
                    name,
                    keys.iter()
                        .map(|(label, key)| (*label, self.value(key).to_owned()))
                        .collect(),
                )
            })
            .collect()
    }
}

pub fn capacity(kib: u64) -> String {
    if kib >= 1024 * 1024 {
        format!("{} GiB", compact(kib as f64 / (1024.0 * 1024.0)))
    } else if kib >= 1024 {
        format!("{} MiB", compact(kib as f64 / 1024.0))
    } else {
        format!("{kib} KiB")
    }
}
fn compact(n: f64) -> String {
    if n.fract() == 0.0 {
        format!("{n:.0}")
    } else {
        format!("{n:.1}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn enums_units_and_missing_strings_are_readable() {
        assert_eq!(
            display_value(&serde_json::json!({"raw":26,"value":"DDR4"})).as_deref(),
            Some("DDR4")
        );
        assert_eq!(
            display_value(&serde_json::json!({"MTs":3200})).as_deref(),
            Some("3200 MT/s")
        );
        assert_eq!(
            display_value(&serde_json::json!("The structure's field is out of bounds")),
            None
        );
        assert_eq!(capacity(32 * 1024 * 1024), "32 GiB");
    }
}
