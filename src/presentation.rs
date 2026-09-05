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
    if let Some(slot) = json.get("SystemSlot") {
        slot_values(slot, &mut result);
    }
    result
}

fn slot_values(slot: &Value, result: &mut BTreeMap<String, String>) {
    // Keep the complete original representation in Raw record/export.
    let number = |key: &str| slot[key]["Number"].as_u64();
    let segment = if slot["segment_group_number"] == "SingleSegment" {
        Some(0)
    } else {
        number("segment_group_number").filter(|n| *n < 65535)
    };
    let bus = number("bus_number").filter(|n| *n < 255);
    let device = slot["device_function_number"]["Number"]["device"]
        .as_u64()
        .filter(|n| *n < 32);
    let function = slot["device_function_number"]["Number"]["function"]
        .as_u64()
        .filter(|n| *n < 8);
    if let (Some(segment), Some(bus), Some(device), Some(function)) =
        (segment, bus, device, function)
    {
        result.insert(
            "pci_address".into(),
            format!("{segment:04x}:{bus:02x}:{device:02x}.{function:x}"),
        );
    }
    if let (Some(device), Some(function)) = (device, function) {
        result.insert(
            "device_function_number".into(),
            format!("{device:02x}.{function:x}"),
        );
    }
    if let Some(segment) = segment {
        result.insert("segment_group_number".into(), format!("{segment:04x}"));
    }
    if let Some(bus) = bus {
        result.insert("bus_number".into(), format!("{bus:02x}"));
    }
    let pcie = slot["system_slot_type"]["value"]
        .get("PciExpress")
        .is_some();
    if pcie {
        if let Some(id) = slot["slot_id"].as_array().filter(|id| id.len() == 2) {
            if let (Some(low), Some(0)) = (id[0].as_u64(), id[1].as_u64()) {
                result.insert("slot_id".into(), low.to_string());
            }
        }
    }
    if let Some(pitch) = slot["slot_pitch"].as_u64() {
        result.insert(
            "slot_pitch".into(),
            if pitch == 0 {
                "Not reported".into()
            } else {
                let mm = format!("{:.2}", pitch as f64 / 100.0);
                format!("{} mm", mm.trim_end_matches('0').trim_end_matches('.'))
            },
        );
    }
    if pcie && slot["slot_information"] == 0 {
        result.insert("slot_information".into(), "Not reported".into());
    }
    let features: Vec<_> = ["slot_characteristics_1", "slot_characteristics_2"]
        .iter()
        .filter_map(|key| result.get(*key).cloned())
        .collect();
    if !features.is_empty() {
        result.insert("slot_features".into(), features.join(", "));
    }
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
            if let Some(parts) = map.get("PciExpress").and_then(Value::as_array) {
                return Some(
                    parts
                        .iter()
                        .filter_map(Value::as_str)
                        .filter(|s| *s != "UndefinedSlotWidth")
                        .map(humanize)
                        .collect::<Vec<_>>()
                        .join(" "),
                );
            }
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
                    "Count" | "Cores" | "Threads" | "Uuid" | "Number" => v,
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
        "pci_address" => return "PCI address".into(),
        "system_slot_type" => return "Slot type".into(),
        "slot_data_bus_width" => return "Electrical width".into(),
        "slot_physical_width" => return "Physical width".into(),
        "slot_features" => return "Supported features".into(),
        "supports_hot_plug_devices" => return "Hot-plug supported".into(),
        "supports_power_management_event" => {
            return "Power management events (PME) supported".into()
        }
        "supports_suprise_removal" => return "Surprise removal supported".into(),
        "supports_smbus_signal" => return "SMBus supported".into(),
        "supports_bifurcation" => return "PCIe bifurcation supported".into(),
        "provides33_volts" => return "3.3 V supplied".into(),
        "provides5_volts" => return "5 V supplied".into(),
        "supports_pc_card16" => return "16-bit PC Card supported".into(),
        "supports_card_bus" => return "CardBus supported".into(),
        "flexbus_slot_cxl10_capable" => return "CXL 1.0 capable".into(),
        "flexbus_slot_cxl20_capable" => return "CXL 2.0 capable".into(),
        "flexbus_slot_cxl30_capable" => return "CXL 3.0 capable".into(),
        "SingleSegment" => return "Single segment".into(),
        "NotApplicable" => return "Not applicable".into(),
        "Sff8639" => return "SFF-8639".into(),
        "PCIExpressGen1" => return "PCIe 1.0".into(),
        "PCIExpressGen2" => return "PCIe 2.0".into(),
        "PCIExpressGen3" => return "PCIe 3.0".into(),
        "PCIExpressGen4" => return "PCIe 4.0".into(),
        "PCIExpressGen5" => return "PCIe 5.0".into(),
        "PCIExpressGen6" => return "PCIe 6.0".into(),
        "X1" => return "x1".into(),
        "X2" => return "x2".into(),
        "X4" => return "x4".into(),
        "X8" => return "x8".into(),
        "X12" => return "x12".into(),
        "X16" => return "x16".into(),
        "X32" => return "x32".into(),
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
            9 => format!(
                "{} · {}",
                self.value("system_slot_type"),
                self.value("current_usage")
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
                    ("Type", "system_slot_type"),
                    ("Usage", "current_usage"),
                    ("Electrical width", "slot_data_bus_width"),
                    ("Physical width", "slot_physical_width"),
                    ("Length", "slot_length"),
                    ("Slot ID", "slot_id"),
                    ("PCI address", "pci_address"),
                    ("Supported features", "slot_features"),
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
    fn slot_details_decode_nested_enums_and_pci_address() {
        let mut json = serde_json::json!({"SystemSlot": {
            "system_slot_type": {"raw":166,"value":{"PciExpress":["PCIExpressGen1","X1"]}},
            "slot_data_bus_width": {"raw":8,"value":"X1"},
            "segment_group_number":"SingleSegment", "bus_number":{"Number":0},
            "device_function_number":{"Number":{"device":28,"function":3}},
            "slot_id":[4,0], "slot_pitch":1255,
            "slot_characteristics_2":{"raw":3,"supports_hot_plug_devices":true,
                "supports_power_management_event":true,"supports_bifurcation":false}
        }});
        let shown = values(&json);
        assert_eq!(shown["system_slot_type"], "PCIe 1.0 x1");
        assert_eq!(shown["slot_data_bus_width"], "x1");
        assert_eq!(shown["pci_address"], "0000:00:1c.3");
        assert_eq!(shown["slot_id"], "4");
        assert_eq!(shown["slot_pitch"], "12.55 mm");
        assert_eq!(
            shown["slot_features"],
            "Hot-plug supported, Power management events (PME) supported"
        );
        json["SystemSlot"]["bus_number"] = serde_json::json!("NotApplicable");
        json["SystemSlot"]["slot_pitch"] = serde_json::json!(0);
        let shown = values(&json);
        assert!(!shown.contains_key("pci_address"));
        assert_eq!(shown["slot_pitch"], "Not reported");
    }
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
