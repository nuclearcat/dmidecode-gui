use dmidecode_rs::{
    smbios::{Handle, SMBiosSystemInformation},
    Inventory,
};

fn table() -> Vec<u8> {
    vec![
        1, 8, 0x34, 0x12, 1, 2, 0, 0, b'A', 0, b'B', 0, 0, 0xf8, 9, 0x35, 0x12, 0xa5, 1, b'S',
        b'U', 1, b'B', b'D', b'F', b'_', 0, 0, 127, 4, 0xff, 0xff, 0, 0,
    ]
}

#[test]
fn typed_access_records_and_oem_are_available_without_cli() {
    let inventory = Inventory::from_bytes(table(), None);
    let system = inventory
        .data
        .first::<SMBiosSystemInformation<'_>>()
        .unwrap();
    assert_eq!(system.manufacturer().to_utf8_lossy().as_deref(), Some("A"));
    assert!(inventory.data.find_by_handle(&Handle(0x1234)).is_some());
    let records = inventory.records().unwrap();
    assert_eq!(records.len(), 3);
    assert_eq!(records[0].handle, 0x1234);
    assert_eq!(records[0].strings, ["A", "B"]);
    assert!(matches!(
        records[1].oem,
        Some(dmidecode_rs::oem::OemDecoded::QualcommWifi { .. })
    ));
}

#[test]
fn file_and_memory_decoding_match_json() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("table.bin");
    std::fs::write(&path, table()).unwrap();
    let memory = Inventory::from_bytes(table(), None);
    let file = Inventory::from_dump(&path).unwrap();
    let expected: serde_json::Value =
        serde_json::from_str(&memory.to_json(false).unwrap()).unwrap();
    assert_eq!(
        expected,
        serde_json::from_str::<serde_json::Value>(&file.to_json(true).unwrap()).unwrap()
    );
}

#[test]
fn missing_dump_returns_io_error() {
    let dir = tempfile::tempdir().unwrap();
    assert!(
        matches!(Inventory::from_dump(dir.path().join("missing.bin")), Err(e) if e.kind() == std::io::ErrorKind::NotFound)
    );
}
