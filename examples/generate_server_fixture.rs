//! Generate the entirely synthetic server used in documentation screenshots.
//! Run from the repository root: cargo run --example generate_server_fixture
use std::{fs, io};

fn record(
    table: &mut Vec<u8>,
    kind: u8,
    handle: u16,
    length: usize,
    strings: &[&str],
    fields: &[(usize, &[u8])],
) {
    let mut bytes = vec![0; length];
    bytes[0] = kind;
    bytes[1] = length as u8;
    bytes[2..4].copy_from_slice(&handle.to_le_bytes());
    for &(offset, value) in fields {
        bytes[offset..offset + value.len()].copy_from_slice(value);
    }
    table.extend(bytes);
    for string in strings {
        table.extend(string.as_bytes());
        table.push(0);
    }
    table.push(0);
    if strings.is_empty() {
        table.push(0);
    }
}

fn main() -> io::Result<()> {
    let mut table = Vec::new();
    record(
        &mut table,
        1,
        0x100,
        27,
        &[
            "Demo Systems",
            "Atlas R128 — Simulated server",
            "Rev 2.0",
            "SIM-SERVER-0001",
            "ATLAS-128",
            "Cloud & virtualization",
        ],
        &[(4, &[1, 2, 3, 4]), (8, &[0x11; 16]), (24, &[6, 5, 6])],
    );
    record(
        &mut table,
        0,
        0,
        26,
        &["Demo UEFI Firmware", "5.14.0", "08/20/2026"],
        &[
            (4, &[1, 2]),
            (8, &[3, 255]),
            (10, &((1u64 << 7) | (1 << 11)).to_le_bytes()),
            (18, &[1, 8, 5, 14]),
            (24, &64u16.to_le_bytes()),
        ],
    );
    record(
        &mut table,
        2,
        0x200,
        15,
        &[
            "Demo Systems",
            "SP5 Server Platform",
            "2.0",
            "SIM-BOARD-0001",
        ],
        &[
            (4, &[1, 2, 3, 4]),
            (10, &[1]),
            (11, &0x300u16.to_le_bytes()),
            (13, &[10]),
        ],
    );
    record(
        &mut table,
        3,
        0x300,
        9,
        &["Demo Systems", "2U rack enclosure", "SIM-CHASSIS-0001"],
        &[(4, &[1, 23, 2, 3, 0])],
    );
    // The legacy thread-count byte uses the sentinel; the extended field holds 256.
    record(
        &mut table,
        4,
        0x400,
        48,
        &[
            "CPU0 / SP5",
            "Advanced Micro Devices, Inc.",
            "AMD EPYC 9754",
            "SIM-CPU-0001",
            "EPYC-9754-DEMO",
        ],
        &[
            (4, &[1, 3, 0xfe, 2]),
            (16, &[3]),
            (20, &3100u16.to_le_bytes()),
            (22, &2250u16.to_le_bytes()),
            (24, &[65, 1]),
            (26, &[0xff; 6]),
            (32, &[4, 0, 5]),
            (35, &[128, 128, 255]),
            (38, &0xfcu16.to_le_bytes()),
            (40, &0x6bu16.to_le_bytes()),
            (42, &128u16.to_le_bytes()),
            (44, &128u16.to_le_bytes()),
            (46, &256u16.to_le_bytes()),
        ],
    );
    record(
        &mut table,
        16,
        0x1000,
        23,
        &[],
        &[
            (4, &[3, 3, 6]),
            (7, &0x80000000u32.to_le_bytes()),
            (11, &0xfffeu16.to_le_bytes()),
            (13, &12u16.to_le_bytes()),
            (15, &(6u64 * 1024 * 1024 * 1024 * 1024).to_le_bytes()),
        ],
    );
    for i in 0u16..12 {
        let locator = format!("CPU0_DIMM_{}1", char::from(b'A' + i as u8));
        let bank = format!("Channel {}", i);
        let serial = format!("SIM-DIMM-{i:04}");
        record(
            &mut table,
            17,
            0x1100 + i,
            40,
            &[
                &locator,
                &bank,
                "Demo Memory",
                &serial,
                "DDR5-128G-ECC-RDIMM",
            ],
            &[
                (4, &0x1000u16.to_le_bytes()),
                (6, &0xfffeu16.to_le_bytes()),
                (8, &80u16.to_le_bytes()),
                (10, &64u16.to_le_bytes()),
                (12, &0x7fffu16.to_le_bytes()),
                (14, &[9]),
                (16, &[1, 2, 34]),
                (19, &(1u16 << 7 | 1 << 13).to_le_bytes()),
                (21, &4800u16.to_le_bytes()),
                (23, &[3, 4, 0, 5]),
                (28, &131072u32.to_le_bytes()),
                (32, &4800u16.to_le_bytes()),
                (38, &1100u16.to_le_bytes()),
            ],
        );
    }
    for i in 0u16..4 {
        let name = format!("PCIe expansion slot {}", i + 1);
        record(
            &mut table,
            9,
            0x900 + i,
            17,
            &[&name],
            &[(4, &[1, 0xc6, 13, 4, 4]), (9, &i.to_le_bytes())],
        );
    }
    record(&mut table, 127, 0xffff, 4, &[], &[]);
    fs::write("tests/fixtures/server.bin", table)
}
