use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::PathBuf;

use crate::profile::Profile;

pub trait HidSink {
    fn write(&mut self, data: &[u8]) -> io::Result<()>;
}

pub struct HidrawDevice {
    file: File,
    report_len: usize,
}

impl HidrawDevice {
    pub fn open(profile: &Profile) -> io::Result<Self> {
        let path = find_hidraw(profile.vid, profile.pid)?;
        let file = OpenOptions::new().write(true).open(path)?;
        Ok(Self { file, report_len: profile.packet.len })
    }
}

impl HidSink for HidrawDevice {
    fn write(&mut self, data: &[u8]) -> io::Result<()> {
        let mut report = data.to_vec();
        report.resize(self.report_len, 0);
        self.file.write_all(&report)
    }
}

fn find_hidraw(vid: u16, pid: u16) -> io::Result<PathBuf> {
    let want = format!("HID_ID=0003:{vid:08X}:{pid:08X}");
    for entry in fs::read_dir("/sys/class/hidraw")? {
        let entry = entry?;
        let uevent = fs::read_to_string(entry.path().join("device/uevent")).unwrap_or_default();
        if uevent.lines().any(|l| l.eq_ignore_ascii_case(&want)) {
            return Ok(PathBuf::from("/dev").join(entry.file_name()));
        }
    }
    Err(io::Error::new(io::ErrorKind::NotFound, format!("no hidraw device {vid:04x}:{pid:04x}")))
}
