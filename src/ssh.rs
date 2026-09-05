//! Read Linux firmware over OpenSSH without installing anything remotely.
use crate::{Inventory, Snapshot};
use dmidecode_rs::smbios::{SMBiosEntryPoint32, SMBiosEntryPoint64, SMBiosVersion};
use std::{
    io::{self, Read},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    thread,
    time::{Duration, Instant},
};

const READ_COMMAND: &str =
    "cat /sys/firmware/dmi/tables/smbios_entry_point /sys/firmware/dmi/tables/DMI";
const MAX_OUTPUT: usize = 16 * 1024 * 1024;
const MAX_ERROR: usize = 32 * 1024;

#[derive(Clone, Debug)]
pub struct Connection {
    pub target: String,
    pub port: Option<u16>,
    pub sudo: bool,
}
impl Connection {
    pub fn new(target: &str, port: &str, sudo: bool) -> Result<Self, String> {
        let target = target.trim();
        if target.is_empty()
            || target.starts_with('-')
            || target.len() > 255
            || !target
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-:@%+[]".contains(&b))
        {
            return Err("Enter a host alias, hostname, IP address, or user@host (without spaces or SSH options).".into());
        }
        let port =
            if port.trim().is_empty() {
                None
            } else {
                Some(port.trim().parse::<u16>().ok().filter(|p| *p > 0).ok_or(
                    "Port must be between 1 and 65535, or blank to use SSH configuration.",
                )?)
            };
        Ok(Self {
            target: target.into(),
            port,
            sudo,
        })
    }
    fn command(&self) -> Command {
        let mut cmd = Command::new("ssh");
        // Keep binary stdout intact and never prompt invisibly in a desktop app.
        cmd.args([
            "-T",
            "-x",
            "-o",
            "BatchMode=yes",
            "-o",
            "StrictHostKeyChecking=yes",
            "-o",
            "ConnectTimeout=10",
            "-o",
            "ConnectionAttempts=1",
            "-o",
            "ClearAllForwardings=yes",
            "-o",
            "ForwardAgent=no",
            "-o",
            "ForkAfterAuthentication=no",
            "-o",
            "RemoteCommand=none",
        ]);
        if let Some(port) = self.port {
            cmd.arg("-p").arg(port.to_string());
        }
        cmd.arg("--").arg(&self.target).arg(if self.sudo {
            format!("sudo -n {READ_COMMAND}")
        } else {
            READ_COMMAND.into()
        });
        cmd
    }
    pub fn fetch(&self, cancel: Arc<AtomicBool>) -> Result<Snapshot, String> {
        let data = capture(self.command(), cancel, Duration::from_secs(45))?;
        let mut inventory = decode_stream(&data)?;
        inventory.source = format!(
            "SSH: {}{}{}",
            self.target,
            self.port
                .map(|p| format!(" (port {p})"))
                .unwrap_or_default(),
            if self.sudo {
                " · sudo firmware read"
            } else {
                ""
            }
        );
        let mut snapshot = Snapshot::decode(inventory)?;
        snapshot.hostname = Some(
            self.target
                .rsplit('@')
                .next()
                .unwrap_or(&self.target)
                .to_owned(),
        );
        Ok(snapshot)
    }
}

fn read_limited(reader: impl Read, limit: usize) -> io::Result<Vec<u8>> {
    let mut data = Vec::new();
    reader.take(limit as u64 + 1).read_to_end(&mut data)?;
    if data.len() > limit {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "SSH output exceeded the size limit",
        ));
    }
    Ok(data)
}

fn capture(
    mut command: Command,
    cancel: Arc<AtomicBool>,
    timeout: Duration,
) -> Result<Vec<u8>, String> {
    if cancel.load(Ordering::Relaxed) {
        return Err("SSH read cancelled.".into());
    }
    let mut child = command.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped())
        .spawn().map_err(|e| format!("Could not start OpenSSH. Install the local ssh client and make sure it is on PATH. {e}"))?;
    let (tx, rx) = mpsc::channel();
    let out = child.stdout.take().expect("stdout is piped");
    let err = child.stderr.take().expect("stderr is piped");
    let out_tx = tx.clone();
    thread::spawn(move || {
        let _ = out_tx.send((true, read_limited(out, MAX_OUTPUT)));
    });
    thread::spawn(move || {
        let _ = tx.send((false, read_limited(err, MAX_ERROR)));
    });
    let started = Instant::now();
    let mut stdout = None;
    let mut stderr = None;
    let result = loop {
        if cancel.load(Ordering::Relaxed) {
            break Err("SSH read cancelled.".into());
        }
        if started.elapsed() >= timeout {
            break Err(
                "SSH read timed out. Check connectivity and the remote host's availability.".into(),
            );
        }
        match rx.recv_timeout(Duration::from_millis(25)) {
            Ok((is_stdout, Ok(data))) => {
                if is_stdout {
                    stdout = Some(data)
                } else {
                    stderr = Some(data)
                }
            }
            Ok((_, Err(e))) => break Err(e.to_string()),
            Err(mpsc::RecvTimeoutError::Timeout) => (),
            Err(mpsc::RecvTimeoutError::Disconnected) if stdout.is_none() || stderr.is_none() => {
                break Err("SSH output reader stopped unexpectedly.".into())
            }
            Err(_) => thread::sleep(Duration::from_millis(25)),
        }
        match child.try_wait() {
            Ok(Some(status)) if stdout.is_some() && stderr.is_some() => {
                if status.success() {
                    break Ok(stdout.take().unwrap());
                }
                let diagnostic = String::from_utf8_lossy(stderr.as_ref().unwrap());
                break Err(format!("SSH firmware read failed ({status}).\n{}\nUse a known host with key/agent authentication. For sudo, allow this read without a password; no terminal or password prompt is opened.", diagnostic.trim()));
            }
            Err(e) => break Err(format!("Could not wait for SSH: {e}")),
            _ => (),
        }
    };
    if result.is_err() {
        let _ = child.kill();
    }
    let _ = child.wait();
    result
}

/// The remote cat writes the entry point first, followed immediately by the table.
fn decode_stream(bytes: &[u8]) -> Result<Inventory, String> {
    let invalid = |reason: &str| {
        format!("Invalid remote SMBIOS data: {reason}. Check that the remote shell does not print text to stdout.")
    };
    let (offset, version, size, exact) = if bytes.starts_with(b"_SM3_") {
        let length = *bytes
            .get(6)
            .ok_or_else(|| invalid("truncated entry point"))? as usize;
        if length < 24 || length > bytes.len() {
            return Err(invalid("invalid SMBIOS 3 entry-point length"));
        }
        let entry = SMBiosEntryPoint64::try_from(bytes[..length].to_vec())
            .map_err(|e| invalid(&e.to_string()))?;
        (
            length,
            SMBiosVersion::new(entry.major_version(), entry.minor_version(), entry.docrev()),
            entry.structure_table_maximum_size() as usize,
            false,
        )
    } else if bytes.starts_with(b"_SM_") {
        // SMBIOS 2.x has a 31-byte entry point. Some old firmware declares 30.
        // The upstream parser expects exactly 31 bytes for the intermediate checksum.
        if bytes.len() < 31 || !matches!(bytes[5], 30 | 31) {
            return Err(invalid("invalid SMBIOS 2 entry-point length"));
        }
        let entry = SMBiosEntryPoint32::try_from(bytes[..31].to_vec())
            .map_err(|e| invalid(&e.to_string()))?;
        (
            31,
            SMBiosVersion::new(entry.major_version(), entry.minor_version(), 0),
            entry.structure_table_length() as usize,
            true,
        )
    } else {
        return Err(invalid("missing SMBIOS entry-point signature"));
    };
    let table = &bytes[offset..];
    if table.is_empty() || table.len() > size || (exact && table.len() != size) {
        return Err(invalid("table length does not match the entry point"));
    }
    let terminated = validate_table(table).map_err(&invalid)?;
    if !exact && !terminated {
        return Err(invalid("missing end-of-table record"));
    }
    Ok(Inventory::from_bytes(table.to_vec(), Some(version)))
}
fn validate_table(mut data: &[u8]) -> Result<bool, &'static str> {
    while !data.is_empty() {
        if data.len() < 4 || data[1] < 4 || data[1] as usize > data.len() {
            return Err("truncated record header or formatted data");
        }
        let formatted = data[1] as usize;
        let end = data[formatted..]
            .windows(2)
            .position(|w| w == [0, 0])
            .ok_or("unterminated record strings")?;
        let tail = &data[formatted + end + 2..];
        if data[0] == 127 {
            return if tail.iter().all(|b| *b == 0) {
                Ok(true)
            } else {
                Err("unexpected data after end-of-table record")
            };
        }
        data = tail;
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn stream3() -> Vec<u8> {
        let table = include_bytes!("../tests/fixtures/server.bin");
        let mut entry = vec![0; 24];
        entry[..5].copy_from_slice(b"_SM3_");
        entry[6] = 24;
        entry[7] = 3;
        entry[8] = 6;
        entry[10] = 1;
        entry[12..16].copy_from_slice(&(table.len() as u32).to_le_bytes());
        entry[5] = 0u8.wrapping_sub(entry.iter().fold(0u8, |s, b| s.wrapping_add(*b)));
        entry.extend(table);
        entry
    }
    #[test]
    fn decodes_binary_stream_with_version_and_extended_fields() {
        let inv = decode_stream(&stream3()).unwrap();
        assert_eq!(inv.data.version.unwrap().major, 3);
        let s = Snapshot::decode(inv).unwrap();
        assert_eq!(
            s.records
                .iter()
                .find(|r| r.kind == 4)
                .unwrap()
                .value("thread_count"),
            "256"
        );
    }
    #[test]
    fn decodes_legacy_entry_point() {
        let table = include_bytes!("../tests/fixtures/workstation.bin");
        let mut entry = vec![0; 31];
        entry[..4].copy_from_slice(b"_SM_");
        entry[5] = 31;
        entry[6] = 2;
        entry[7] = 8;
        entry[16..21].copy_from_slice(b"_DMI_");
        entry[22..24].copy_from_slice(&(table.len() as u16).to_le_bytes());
        entry[21] = 0u8.wrapping_sub(entry[16..].iter().fold(0u8, |s, b| s.wrapping_add(*b)));
        entry[4] = 0u8.wrapping_sub(entry.iter().fold(0u8, |s, b| s.wrapping_add(*b)));
        entry.extend(table);
        assert_eq!(
            decode_stream(&entry).unwrap().data.version.unwrap().minor,
            8
        );
    }
    #[test]
    fn rejects_corruption_and_truncation_without_panicking() {
        let data = stream3();
        for n in 0..data.len() {
            assert!(decode_stream(&data[..n]).is_err(), "accepted prefix {n}");
        }
        let mut bad = data.clone();
        bad[5] ^= 1;
        assert!(decode_stream(&bad).is_err());
        let mut bad = data.clone();
        bad[25] = 3;
        assert!(decode_stream(&bad).is_err());
        assert!(decode_stream(b"Welcome to server!\n").is_err());
    }
    #[test]
    fn host_and_port_are_arguments_not_shell_commands() {
        for invalid in [
            "",
            "-oProxyCommand=id",
            "host;id",
            "host name",
            "host\nid",
            "$(id)",
        ] {
            assert!(Connection::new(invalid, "", false).is_err());
        }
        assert!(Connection::new("host", "0", false).is_err());
        assert!(Connection::new("host", "65536", false).is_err());
        let c = Connection::new("admin@server-alias", "2222", true).unwrap();
        let args: Vec<_> = c
            .command()
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            &args[args.len() - 3..],
            &[
                "--",
                "admin@server-alias",
                &format!("sudo -n {READ_COMMAND}")
            ]
        );
        assert!(args.windows(2).any(|w| w == ["-p", "2222"]));
    }
    #[test]
    fn bounds_output() {
        assert!(read_limited(&b"12345"[..], 4).is_err());
    }
    #[cfg(unix)]
    #[test]
    fn subprocess_handles_binary_errors_timeout_and_cancellation() {
        let cancel = Arc::new(AtomicBool::new(false));
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "printf '\\001\\000\\377'"]);
        assert_eq!(
            capture(cmd, cancel.clone(), Duration::from_secs(2)).unwrap(),
            [1, 0, 255]
        );
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "printf 'access denied' >&2; exit 1"]);
        assert!(capture(cmd, cancel.clone(), Duration::from_secs(2))
            .unwrap_err()
            .contains("access denied"));
        let mut cmd = Command::new("sleep");
        cmd.arg("2");
        assert!(capture(cmd, cancel.clone(), Duration::from_millis(50))
            .unwrap_err()
            .contains("timed out"));
        let flag = cancel.clone();
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(50));
            flag.store(true, Ordering::Relaxed);
        });
        let mut cmd = Command::new("sleep");
        cmd.arg("2");
        assert!(capture(cmd, cancel, Duration::from_secs(2))
            .unwrap_err()
            .contains("cancelled"));
    }
}
