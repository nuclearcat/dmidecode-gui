use crate::LoadOptions;
use smbioslib::*;
use std::fmt::Write;
use std::io::Error;

pub fn table_load(_opt: &LoadOptions) -> Result<(SMBiosData, String), Error> {
    let mut output = String::new();

    writeln!(&mut output, "Getting SMBIOS data from IOKit.").unwrap();

    let smbios_table = table_load_from_device()?;

    Ok((smbios_table, output))
}
