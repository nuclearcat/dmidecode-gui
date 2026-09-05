//! Decoders for OEM structures whose layouts are published outside SMBIOS.

use smbioslib::{SMBiosData, SMBiosSystemInformation, UndefinedStruct};
use std::convert::TryInto;

#[derive(Debug, Default)]
pub struct OemContext {
    manufacturer: Option<String>,
}

impl OemContext {
    pub fn from_data(data: &SMBiosData) -> Self {
        Self {
            manufacturer: data
                .find_map(|info: SMBiosSystemInformation<'_>| info.manufacturer().to_utf8_lossy()),
        }
    }

    fn is_apple(&self) -> bool {
        self.manufacturer
            .as_deref()
            .map(str::trim)
            .map(|manufacturer| {
                manufacturer.eq_ignore_ascii_case("Apple")
                    || manufacturer.to_ascii_lowercase().starts_with("apple ")
            })
            .unwrap_or(false)
    }
}

#[derive(Debug, PartialEq)]
pub enum OemDecoded {
    QualcommWifi {
        features_disabled: u8,
        country_code_mode: u8,
        country_code: [u8; 2],
        board_data_file: String,
    },
    IntelWifiMarker,
    IntelVproVerification {
        structure_version: u32,
        cpu_capabilities: u32,
        mch_capabilities: u64,
        ich_capabilities: u32,
        me_capabilities: u32,
        tpm_capabilities: u32,
        network_devices: [u8; 12],
        bios_capabilities: u32,
    },
    IntelFirmwareVersionInfo(Vec<IntelFirmwareVersion>),
    AppleFirmwareInformation {
        region_count: u8,
        firmware_features: u32,
        firmware_features_mask: u32,
        regions: Vec<AppleFirmwareRegion>,
        extended_firmware_features: Option<u32>,
        extended_firmware_features_mask: Option<u32>,
    },
    AppleMemorySpd {
        memory_device_handle: u16,
        offset: u16,
        declared_size: u16,
        data: Vec<u8>,
    },
    AppleProcessorType(u16),
    AppleProcessorBusSpeed(u16),
    ApplePlatformFeature(u64),
    AppleSmcInformation(Vec<u8>),
}

#[derive(Debug, PartialEq)]
pub struct AppleFirmwareRegion {
    pub region_type: u8,
    pub start_address: u32,
    pub end_address: u32,
}

#[derive(Debug, PartialEq)]
pub struct IntelFirmwareVersion {
    pub component_name: String,
    pub version_string: Option<String>,
    pub major: u8,
    pub minor: u8,
    pub revision: u8,
    pub build: u16,
}

pub fn decode_oem(data: &UndefinedStruct, context: &OemContext) -> Option<OemDecoded> {
    decode_qualcomm_wifi(data)
        .or_else(|| decode_intel_wifi_marker(data))
        .or_else(|| decode_intel_vpro(data))
        .or_else(|| decode_intel_fvi(data))
        .or_else(|| context.is_apple().then(|| decode_apple(data)).flatten())
}

fn decode_qualcomm_wifi(data: &UndefinedStruct) -> Option<OemDecoded> {
    if data.header.struct_type() != 0xF8 || data.fields.len() != 0x09 {
        return None;
    }

    let board_data_file = data.strings.iter().next()?;
    if !board_data_file.starts_with(b"BDF_") {
        return None;
    }

    Some(OemDecoded::QualcommWifi {
        features_disabled: data.get_field_byte(0x04)?,
        country_code_mode: data.get_field_byte(0x05)?,
        // Linux treats this native-endian packed WORD as two network-order
        // ASCII characters (for example the bytes 53 55 represent "US").
        country_code: [data.get_field_byte(0x07)?, data.get_field_byte(0x06)?],
        board_data_file: String::from_utf8_lossy(board_data_file).into_owned(),
    })
}

fn decode_intel_wifi_marker(data: &UndefinedStruct) -> Option<OemDecoded> {
    if data.header.struct_type() == 0x85
        && data.fields.len() == 0x05
        && data.get_field_byte(0x04) == Some(1)
        && data.strings.iter().next().map(Vec::as_slice) == Some(b"KHOIHGIUCCHHII")
    {
        Some(OemDecoded::IntelWifiMarker)
    } else {
        None
    }
}

fn decode_intel_vpro(data: &UndefinedStruct) -> Option<OemDecoded> {
    if data.header.struct_type() != 0x83
        || data.fields.len() != 0x40
        || data.get_field_data(0x38, 0x3C) != Some(b"vPro")
    {
        return None;
    }

    Some(OemDecoded::IntelVproVerification {
        structure_version: data.get_field_dword(0x0C)?,
        cpu_capabilities: data.get_field_dword(0x10)?,
        mch_capabilities: data.get_field_qword(0x14)?,
        ich_capabilities: data.get_field_dword(0x1C)?,
        me_capabilities: data.get_field_dword(0x20)?,
        tpm_capabilities: data.get_field_dword(0x24)?,
        network_devices: data.get_field_data(0x28, 0x34)?.try_into().ok()?,
        bios_capabilities: data.get_field_dword(0x34)?,
    })
}

fn decode_intel_fvi(data: &UndefinedStruct) -> Option<OemDecoded> {
    if data.header.struct_type() != 0xDD || data.fields.len() < 0x05 {
        return None;
    }

    let count = usize::from(data.get_field_byte(0x04)?);
    if count == 0 || data.fields.len() != 0x05 + count * 7 {
        return None;
    }

    let mut entries = Vec::with_capacity(count);
    for index in 0..count {
        let offset = 0x05 + index * 7;
        let component_index = data.get_field_byte(offset)?;
        if component_index == 0 {
            return None;
        }
        let component_name = data
            .strings
            .get_string(component_index)
            .to_utf8_lossy()?
            .trim()
            .to_string();
        if component_name.is_empty() {
            return None;
        }

        let version_index = data.get_field_byte(offset + 1)?;
        let version_string = if version_index == 0 {
            None
        } else {
            data.strings
                .get_string(version_index)
                .to_utf8_lossy()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        };
        entries.push(IntelFirmwareVersion {
            component_name,
            version_string,
            major: data.get_field_byte(offset + 2)?,
            minor: data.get_field_byte(offset + 3)?,
            revision: data.get_field_byte(offset + 4)?,
            build: data.get_field_word(offset + 5)?,
        });
    }

    // Type 0xDD is also used by other OEMs. Require a component name used by
    // Intel's FVI producers so an unrelated, structurally similar record is
    // not decoded as Intel firmware information.
    if !entries
        .iter()
        .any(|entry| is_intel_fvi_component(&entry.component_name))
    {
        return None;
    }

    Some(OemDecoded::IntelFirmwareVersionInfo(entries))
}

fn is_intel_fvi_component(name: &str) -> bool {
    [
        "Reference Code",
        "uCode Version",
        "TXT ACM version",
        "FSP Binary Version",
        "BIOS Guard",
        "ME Firmware Version",
        "MEBx version",
        "PCH-CRID",
        "SA-CRID",
        "PCH Hsio Version",
        "GOP Version",
        "EC FW Version",
        "Client Silicon Version",
    ]
    .iter()
    .any(|known| name.contains(known))
}

fn decode_apple(data: &UndefinedStruct) -> Option<OemDecoded> {
    match data.header.struct_type() {
        128 => decode_apple_firmware(data),
        130 => {
            if data.fields.len() < 0x0A {
                return None;
            }
            let declared_size = data.get_field_word(0x08)?;
            let available = data.fields.len() - 0x0A;
            let size = usize::from(declared_size).min(available);
            Some(OemDecoded::AppleMemorySpd {
                memory_device_handle: data.get_field_word(0x04)?,
                offset: data.get_field_word(0x06)?,
                declared_size,
                data: data.fields[0x0A..0x0A + size].to_vec(),
            })
        }
        131 if data.fields.len() >= 0x06 => {
            Some(OemDecoded::AppleProcessorType(data.get_field_word(0x04)?))
        }
        132 if data.fields.len() >= 0x06 => Some(OemDecoded::AppleProcessorBusSpeed(
            data.get_field_word(0x04)?,
        )),
        133 if data.fields.len() >= 0x0C => Some(OemDecoded::ApplePlatformFeature(
            data.get_field_qword(0x04)?,
        )),
        134 if data.fields.len() >= 0x14 => Some(OemDecoded::AppleSmcInformation(
            data.fields[0x04..0x14].to_vec(),
        )),
        _ => None,
    }
}

fn decode_apple_firmware(data: &UndefinedStruct) -> Option<OemDecoded> {
    const BASE_LENGTH: usize = 0x58;
    const EXTENDED_LENGTH: usize = 0x60;
    if data.fields.len() < BASE_LENGTH {
        return None;
    }

    let region_count = data.get_field_byte(0x04)?;
    let map_count = usize::from(region_count).min(8);
    let mut regions = Vec::with_capacity(map_count);
    for index in 0..map_count {
        let map_offset = 0x18 + index * 8;
        regions.push(AppleFirmwareRegion {
            region_type: data.get_field_byte(0x10 + index)?,
            start_address: data.get_field_dword(map_offset)?,
            end_address: data.get_field_dword(map_offset + 4)?,
        });
    }

    Some(OemDecoded::AppleFirmwareInformation {
        region_count,
        firmware_features: data.get_field_dword(0x08)?,
        firmware_features_mask: data.get_field_dword(0x0C)?,
        regions,
        extended_firmware_features: (data.fields.len() >= EXTENDED_LENGTH)
            .then(|| data.get_field_dword(0x58))
            .flatten(),
        extended_firmware_features_mask: (data.fields.len() >= EXTENDED_LENGTH)
            .then(|| data.get_field_dword(0x5C))
            .flatten(),
    })
}

impl OemDecoded {
    pub fn print(&self) {
        match self {
            OemDecoded::QualcommWifi {
                features_disabled,
                country_code_mode,
                country_code,
                board_data_file,
            } => {
                println!("Qualcomm Wi-Fi Board Data");
                println!("\tFeatures Disabled: {:#04X}", features_disabled);
                let mode = match country_code_mode {
                    0 => "Disabled",
                    1 => "ISO 3166-1 alpha-2",
                    2 => "Worldwide",
                    _ => "Unknown",
                };
                println!("\tCountry Code Mode: {} ({:#04X})", mode, country_code_mode);
                if *country_code_mode == 1 && country_code.iter().all(u8::is_ascii_alphabetic) {
                    println!(
                        "\tCountry Code: {}{}",
                        country_code[0] as char, country_code[1] as char
                    );
                }
                println!("\tBoard Data File: {}", board_data_file);
            }
            OemDecoded::IntelWifiMarker => println!("Intel Wi-Fi Compatibility Marker"),
            OemDecoded::IntelVproVerification {
                structure_version,
                cpu_capabilities,
                mch_capabilities,
                ich_capabilities,
                me_capabilities,
                tpm_capabilities,
                network_devices,
                bios_capabilities,
            } => {
                println!("Intel vPro Verification Table");
                println!(
                    "\tStructure Version: {}.{} ({:#010X})",
                    structure_version >> 16,
                    structure_version & 0xFFFF,
                    structure_version
                );
                println!("\tCPU Capabilities: {:#010X}", cpu_capabilities);
                print_set_capability(*cpu_capabilities, 0, "VMX enabled");
                print_set_capability(*cpu_capabilities, 1, "SMX enabled");
                print_set_capability(*cpu_capabilities, 2, "TXT capable");
                print_set_capability(*cpu_capabilities, 3, "TXT enabled");
                print_set_capability(*cpu_capabilities, 4, "VT-x capable");
                print_set_capability(*cpu_capabilities, 5, "VT-x enabled");
                println!("\tMCH Capabilities: {:#018X}", mch_capabilities);
                println!("\tICH Capabilities: {:#010X}", ich_capabilities);
                println!("\tME Capabilities: {:#010X}", me_capabilities);
                print_set_capability(*me_capabilities, 0, "ME enabled");
                print_set_capability(*me_capabilities, 1, "Intel QST supported");
                print_set_capability(*me_capabilities, 2, "Intel ASF supported");
                print_set_capability(*me_capabilities, 3, "Intel AMT supported");
                println!("\tTPM Capabilities: {:#010X}", tpm_capabilities);
                println!("\tNetwork Device Data: {}", format_hex(network_devices));
                println!("\tBIOS Capabilities: {:#010X}", bios_capabilities);
                print_set_capability(*bios_capabilities, 0, "VT-x BIOS setting supported");
                print_set_capability(*bios_capabilities, 1, "VT-d BIOS setting supported");
                print_set_capability(*bios_capabilities, 2, "TXT BIOS setting supported");
                print_set_capability(*bios_capabilities, 3, "TPM BIOS setting supported");
                print_set_capability(*bios_capabilities, 4, "ME BIOS setting supported");
                print_set_capability(*bios_capabilities, 5, "VA extensions supported");
            }
            OemDecoded::IntelFirmwareVersionInfo(entries) => {
                println!("Intel Firmware Version Information");
                for entry in entries {
                    print!("\t{}: ", entry.component_name);
                    if let Some(version) = &entry.version_string {
                        println!("{}", version);
                    } else if entry.major == 0xFF
                        && entry.minor == 0xFF
                        && entry.revision == 0xFF
                        && entry.build == 0xFFFF
                    {
                        println!("Unknown");
                    } else {
                        println!(
                            "{}.{}.{}.{}",
                            entry.major, entry.minor, entry.revision, entry.build
                        );
                    }
                }
            }
            OemDecoded::AppleFirmwareInformation {
                region_count,
                firmware_features,
                firmware_features_mask,
                regions,
                extended_firmware_features,
                extended_firmware_features_mask,
            } => {
                println!("Apple Firmware Information");
                println!("\tRegion Count: {}", region_count);
                println!("\tFirmware Features: {:#010X}", firmware_features);
                println!("\tFirmware Features Mask: {:#010X}", firmware_features_mask);
                for (index, region) in regions.iter().enumerate() {
                    println!(
                        "\tRegion {}: {} ({:#04X}), {:#010X}-{:#010X}",
                        index + 1,
                        apple_region_type(region.region_type),
                        region.region_type,
                        region.start_address,
                        region.end_address
                    );
                }
                if let Some(value) = extended_firmware_features {
                    println!("\tExtended Firmware Features: {:#010X}", value);
                }
                if let Some(value) = extended_firmware_features_mask {
                    println!("\tExtended Firmware Features Mask: {:#010X}", value);
                }
            }
            OemDecoded::AppleMemorySpd {
                memory_device_handle,
                offset,
                declared_size,
                data,
            } => {
                println!("Apple Memory SPD Data");
                println!("\tMemory Device Handle: {:#06X}", memory_device_handle);
                println!("\tOffset: {}", offset);
                println!("\tSize: {}", declared_size);
                println!("\tData: {}", format_hex(data));
            }
            OemDecoded::AppleProcessorType(value) => {
                println!("Apple Processor Type");
                println!("\tType: {:#06X}", value);
            }
            OemDecoded::AppleProcessorBusSpeed(value) => {
                println!("Apple Processor Bus Speed");
                println!("\tBus Speed: {} MHz", value);
            }
            OemDecoded::ApplePlatformFeature(value) => {
                println!("Apple Platform Feature");
                println!("\tFeature Bitmap: {:#018X}", value);
            }
            OemDecoded::AppleSmcInformation(value) => {
                println!("Apple SMC Information");
                let version = value
                    .iter()
                    .copied()
                    .take_while(|byte| *byte != 0)
                    .collect::<Vec<_>>();
                if !version.is_empty() && version.iter().all(u8::is_ascii_graphic) {
                    println!("\tSMC Version: {}", String::from_utf8_lossy(&version));
                } else {
                    println!("\tSMC Version: {}", format_hex(value));
                }
            }
        }
    }
}

fn apple_region_type(value: u8) -> &'static str {
    match value {
        0 => "Reserved",
        1 => "Recovery",
        2 => "Main",
        3 => "NVRAM",
        4 => "Config",
        5 => "DiagVault",
        _ => "Unknown",
    }
}

fn print_set_capability(value: u32, bit: u8, label: &str) {
    if value & (1u32 << bit) != 0 {
        println!("\t\t{}", label);
    }
}

fn format_hex(data: &[u8]) -> String {
    data.iter()
        .map(|byte| format!("{:02X}", byte))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn structure(mut fields: Vec<u8>, strings: &[&[u8]]) -> UndefinedStruct {
        for string in strings {
            fields.extend_from_slice(string);
            fields.push(0);
        }
        fields.push(0);
        UndefinedStruct::new(&fields)
    }

    #[test]
    fn decodes_qualcomm_bdf_record_only_with_signature() {
        let record = structure(
            vec![0xF8, 0x09, 0x34, 0x12, 0xA5, 1, b'S', b'U', 1],
            &[b"BDF_variant"],
        );
        assert_eq!(
            decode_oem(&record, &OemContext::default()),
            Some(OemDecoded::QualcommWifi {
                features_disabled: 0xA5,
                country_code_mode: 1,
                country_code: *b"US",
                board_data_file: "BDF_variant".to_string(),
            })
        );

        let unrecognized = structure(
            vec![0xF8, 0x09, 0x34, 0x12, 0, 1, b'S', b'U', 1],
            &[b"not-a-bdf"],
        );
        assert_eq!(decode_oem(&unrecognized, &OemContext::default()), None);
    }

    #[test]
    fn decodes_exact_intel_wifi_marker() {
        let record = structure(vec![0x85, 0x05, 0, 0, 1], &[b"KHOIHGIUCCHHII"]);
        assert_eq!(
            decode_oem(&record, &OemContext::default()),
            Some(OemDecoded::IntelWifiMarker)
        );
    }

    #[test]
    fn decodes_intel_vpro_verification_table_by_signature() {
        let mut fields = vec![0x83, 0x40, 0x3B, 0x00];
        fields.extend_from_slice(&[0x35, 0, 0, 0, 0, 0, 0, 0]);
        fields.extend_from_slice(&0x0001_0000u32.to_le_bytes());
        fields.extend_from_slice(&0x0000_0039u32.to_le_bytes());
        fields.extend_from_slice(&0x1122_3344_5566_7788u64.to_le_bytes());
        fields.extend_from_slice(&0x0102_0304u32.to_le_bytes());
        fields.extend_from_slice(&0x0000_0009u32.to_le_bytes());
        fields.extend_from_slice(&0x0506_0708u32.to_le_bytes());
        fields.extend_from_slice(&[0; 12]);
        fields.extend_from_slice(&0x0000_0026u32.to_le_bytes());
        fields.extend_from_slice(b"vPro");
        fields.extend_from_slice(&[0; 4]);
        let record = structure(fields, &[]);

        assert!(matches!(
            decode_oem(&record, &OemContext::default()),
            Some(OemDecoded::IntelVproVerification {
                cpu_capabilities: 0x39,
                bios_capabilities: 0x26,
                ..
            })
        ));
    }

    #[test]
    fn decodes_intel_firmware_version_entries() {
        let record = structure(
            vec![
                0xDD, 0x13, 0, 0, 2, 1, 0, 1, 2, 3, 5, 0, 2, 3, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
            ],
            &[b"Reference Code", b"Microcode", b"external-version"],
        );
        assert_eq!(
            decode_oem(&record, &OemContext::default()),
            Some(OemDecoded::IntelFirmwareVersionInfo(vec![
                IntelFirmwareVersion {
                    component_name: "Reference Code".to_string(),
                    version_string: None,
                    major: 1,
                    minor: 2,
                    revision: 3,
                    build: 5,
                },
                IntelFirmwareVersion {
                    component_name: "Microcode".to_string(),
                    version_string: Some("external-version".to_string()),
                    major: 0xFF,
                    minor: 0xFF,
                    revision: 0xFF,
                    build: 0xFFFF,
                },
            ]))
        );
    }

    #[test]
    fn apple_types_are_vendor_gated() {
        let record = structure(vec![131, 6, 0, 0, 0x05, 0x07], &[]);
        assert_eq!(decode_oem(&record, &OemContext::default()), None);

        let context = OemContext {
            manufacturer: Some("Apple Inc.".to_string()),
        };
        assert_eq!(
            decode_oem(&record, &context),
            Some(OemDecoded::AppleProcessorType(0x0705))
        );
    }

    #[test]
    fn truncated_apple_records_remain_undecoded() {
        let context = OemContext {
            manufacturer: Some("Apple Inc.".to_string()),
        };
        let record = structure(vec![134, 5, 0, 0, 1], &[]);
        assert_eq!(decode_oem(&record, &context), None);
    }
}
