//! Optical drives: finding them, their tray and disc status, reading a disc's table of contents,
//! and streaming a track's audio.
//!
//! Set `VIPER_SIMULATED_CD=1` to add a simulated drive holding the table of contents of a real
//! album (Radiohead's In Rainbows), so the ripper can be developed and tried without a drive or a
//! disc. MusicBrainz knows the disc, and its tracks play quiet tones.
use std::{
    io::{self, Read},
    path::PathBuf,
    process::{Child, ChildStdout, Command, Stdio},
    sync::atomic::Ordering,
    time::Duration,
};

/// Bytes in one audio CD sector: 1/75 s of 16-bit stereo PCM at 44.1 kHz.
pub(crate) const SECTOR_BYTES: u64 = 2352;
/// MusicBrainz offsets count from the start of the lead-in, 150 sectors (2 s) before LBA 0.
pub(crate) const LEAD_IN: u32 = 150;
/// On an enhanced CD, the data session starts this many sectors after the audio session ends.
const SESSION_GAP: u32 = 11_400;
/// How much faster than real time the simulated drive reads, close to a real drive's average.
const SIMULATED_SPEED: u64 = 40;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Drive {
    /// A drive the kernel exposes as `/dev/srN`.
    Device { path: PathBuf, name: String },
    /// The development drive enabled by `VIPER_SIMULATED_CD`.
    Simulated,
}

/// What the drive reports about its tray and disc.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DriveStatus {
    TrayOpen,
    NoDisc,
    /// The drive is spinning up or reading a newly inserted disc.
    NotReady,
    DiscPresent,
    /// The device could not be opened, usually for lack of permission.
    Inaccessible(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TocTrack {
    pub(crate) number: u8,
    /// First sector, as a logical block address.
    pub(crate) start: u32,
    pub(crate) audio: bool,
}

/// A disc's table of contents, in logical block addresses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Toc {
    pub(crate) tracks: Vec<TocTrack>,
    pub(crate) leadout: u32,
}

/// One audio track's span on the disc.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AudioTrack {
    pub(crate) number: u8,
    pub(crate) start: u32,
    pub(crate) sectors: u32,
}

impl AudioTrack {
    pub(crate) fn bytes(self) -> u64 {
        u64::from(self.sectors) * SECTOR_BYTES
    }

    pub(crate) fn duration(self) -> Duration {
        Duration::from_millis(u64::from(self.sectors) * 1000 / 75)
    }
}

impl Toc {
    /// The end of the audio session: the lead-out, or on an enhanced CD the start of the data
    /// session less the gap between sessions. MusicBrainz disc IDs use this as the lead-out.
    pub(crate) fn audio_leadout(&self) -> u32 {
        match self.tracks.last() {
            Some(last) if !last.audio && self.tracks.len() > 1 => last.start - SESSION_GAP,
            _ => self.leadout,
        }
    }

    /// The audio tracks with their lengths, in disc order.
    pub(crate) fn audio_tracks(&self) -> Vec<AudioTrack> {
        let end = self.audio_leadout();
        self.tracks
            .iter()
            .enumerate()
            .filter(|(_, track)| track.audio)
            .map(|(index, track)| {
                let next = self
                    .tracks
                    .get(index + 1)
                    .filter(|next| next.audio)
                    .map_or(end, |next| next.start);
                AudioTrack {
                    number: track.number,
                    start: track.start,
                    sectors: next.saturating_sub(track.start),
                }
            })
            .collect()
    }
}

impl Drive {
    /// The drives on this machine, plus the simulated one when `VIPER_SIMULATED_CD` is set.
    pub(crate) fn list() -> Vec<Self> {
        let mut drives = device::devices();
        if std::env::var_os("VIPER_SIMULATED_CD").is_some_and(|value| value != "0") {
            drives.push(Self::Simulated);
        }
        drives
    }

    pub(crate) fn name(&self) -> &str {
        match self {
            Self::Device { name, .. } => name,
            Self::Simulated => "Simulated drive",
        }
    }

    pub(crate) fn status(&self) -> DriveStatus {
        match self {
            Self::Device { path, .. } => device::status(path),
            Self::Simulated if simulated::TRAY_OPEN.load(Ordering::Relaxed) => {
                DriveStatus::TrayOpen
            }
            Self::Simulated => DriveStatus::DiscPresent,
        }
    }

    pub(crate) fn read_toc(&self) -> io::Result<Toc> {
        match self {
            Self::Device { path, .. } => device::read_toc(path),
            Self::Simulated => Ok(simulated::toc()),
        }
    }

    pub(crate) fn eject(&self) -> io::Result<()> {
        match self {
            Self::Device { path, .. } => device::eject(path),
            Self::Simulated => {
                simulated::TRAY_OPEN.store(true, Ordering::Relaxed);
                Ok(())
            }
        }
    }

    pub(crate) fn close_tray(&self) -> io::Result<()> {
        match self {
            Self::Device { path, .. } => device::close_tray(path),
            Self::Simulated => {
                simulated::TRAY_OPEN.store(false, Ordering::Relaxed);
                Ok(())
            }
        }
    }

    /// Start reading one track as raw 16-bit little-endian stereo PCM. Real drives read through
    /// cdparanoia, which re-reads and verifies until each sector is right or it gives up.
    pub(crate) fn open_track(&self, track: AudioTrack) -> io::Result<TrackReader> {
        match self {
            Self::Device { path, .. } => {
                let mut child = Command::new("cdparanoia")
                    .arg("--force-cdrom-device")
                    .arg(path)
                    .args(["--quiet", "--output-raw-little-endian", "--"])
                    .arg(track.number.to_string())
                    .arg("-")
                    .stdin(Stdio::null())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::null())
                    .spawn()
                    .map_err(|error| match error.kind() {
                        io::ErrorKind::NotFound => io::Error::new(
                            io::ErrorKind::NotFound,
                            "cdparanoia is not installed; install it to rip CDs",
                        ),
                        _ => error,
                    })?;
                let stdout = child.stdout.take().ok_or_else(|| {
                    io::Error::other("cdparanoia started without an output stream")
                })?;
                Ok(TrackReader::Paranoia { child, stdout })
            }
            Self::Simulated => Ok(TrackReader::Simulated(simulated::Tone::new(track))),
        }
    }
}

/// Whether cdparanoia, which reads audio from real drives, is installed.
pub(crate) fn paranoia_installed() -> bool {
    std::env::var_os("PATH").is_some_and(|paths| {
        std::env::split_paths(&paths).any(|dir| dir.join("cdparanoia").is_file())
    })
}

/// A track being read from the disc.
pub(crate) enum TrackReader {
    Paranoia { child: Child, stdout: ChildStdout },
    Simulated(simulated::Tone),
}

impl Read for TrackReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::Paranoia { stdout, .. } => stdout.read(buffer),
            Self::Simulated(tone) => tone.read(buffer),
        }
    }
}

impl TrackReader {
    /// Wait for the read to end, and fail when cdparanoia reported an error.
    pub(crate) fn finish(mut self) -> io::Result<()> {
        match &mut self {
            Self::Paranoia { child, stdout } => {
                // Read what is left so cdparanoia never waits on a full pipe.
                io::copy(stdout, &mut io::sink())?;
                let status = child.wait()?;
                if status.success() {
                    Ok(())
                } else {
                    Err(io::Error::other(format!(
                        "cdparanoia could not read the track ({status})"
                    )))
                }
            }
            Self::Simulated(_) => Ok(()),
        }
    }
}

impl Drop for TrackReader {
    /// Stop cdparanoia when a rip is cancelled or fails part way.
    fn drop(&mut self) {
        if let Self::Paranoia { child, .. } = self {
            if matches!(child.try_wait(), Ok(None)) {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }
}

#[cfg(target_os = "linux")]
mod device {
    use super::{Drive, DriveStatus, Toc, TocTrack};
    use std::{
        fs::{self, File, OpenOptions},
        io,
        os::{fd::AsRawFd, unix::fs::OpenOptionsExt},
        path::{Path, PathBuf},
    };

    // From <linux/cdrom.h>.
    const CDROMREADTOCHDR: libc::c_ulong = 0x5305;
    const CDROMREADTOCENTRY: libc::c_ulong = 0x5306;
    const CDROMEJECT: libc::c_ulong = 0x5309;
    const CDROMCLOSETRAY: libc::c_ulong = 0x5319;
    const CDROM_DRIVE_STATUS: libc::c_ulong = 0x5326;
    const CDSL_CURRENT: libc::c_int = libc::c_int::MAX - 1;
    const CDS_NO_DISC: libc::c_int = 1;
    const CDS_TRAY_OPEN: libc::c_int = 2;
    const CDS_DRIVE_NOT_READY: libc::c_int = 3;
    const CDROM_LBA: u8 = 0x01;
    const CDROM_LEADOUT: u8 = 0xAA;
    const CDROM_DATA_TRACK: u8 = 0x04;

    #[repr(C)]
    #[derive(Default)]
    struct TocHeader {
        first: u8,
        last: u8,
    }

    /// `struct cdrom_tocentry`, with `cdte_addr` read as its `lba` member.
    #[repr(C)]
    #[derive(Default)]
    struct TocEntry {
        track: u8,
        /// `cdte_adr` in the low four bits, `cdte_ctrl` in the high four.
        adr_ctrl: u8,
        format: u8,
        lba: libc::c_int,
        datamode: u8,
    }

    pub(super) fn devices() -> Vec<Drive> {
        let Ok(entries) = fs::read_dir("/sys/block") else {
            return Vec::new();
        };
        let mut drives: Vec<Drive> = entries
            .flatten()
            .filter_map(|entry| {
                let device = entry.file_name().to_str()?.to_owned();
                device.starts_with("sr").then(|| {
                    let read = |name: &str| {
                        fs::read_to_string(entry.path().join("device").join(name))
                            .map(|value| value.trim().to_owned())
                            .unwrap_or_default()
                    };
                    let label = [read("vendor"), read("model")]
                        .into_iter()
                        .filter(|part| !part.is_empty())
                        .collect::<Vec<_>>()
                        .join(" ");
                    let path = PathBuf::from("/dev").join(&device);
                    Drive::Device {
                        name: if label.is_empty() {
                            path.display().to_string()
                        } else {
                            label
                        },
                        path,
                    }
                })
            })
            .collect();
        drives.sort_by(|a, b| match (a, b) {
            (Drive::Device { path: a, .. }, Drive::Device { path: b, .. }) => a.cmp(b),
            _ => std::cmp::Ordering::Equal,
        });
        drives
    }

    /// Open without waiting for a disc, so status checks work with the tray open.
    fn open(path: &Path) -> io::Result<File> {
        OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(path)
    }

    fn ioctl<T>(file: &File, request: libc::c_ulong, argument: T) -> io::Result<libc::c_int> {
        // SAFETY: each caller passes the argument type the request expects in <linux/cdrom.h>.
        let result = unsafe { libc::ioctl(file.as_raw_fd(), request as _, argument) };
        if result < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(result)
        }
    }

    pub(super) fn status(path: &Path) -> DriveStatus {
        let file = match open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::PermissionDenied => {
                return DriveStatus::Inaccessible(format!(
                    "Viper Music is not allowed to open {}. Add yourself to the group that owns \
                     it (often \"optical\"), then sign in again.",
                    path.display()
                ));
            }
            Err(error) => {
                return DriveStatus::Inaccessible(format!(
                    "Could not open {}: {error}",
                    path.display()
                ))
            }
        };
        match ioctl(&file, CDROM_DRIVE_STATUS, CDSL_CURRENT) {
            Ok(CDS_NO_DISC) => DriveStatus::NoDisc,
            Ok(CDS_TRAY_OPEN) => DriveStatus::TrayOpen,
            Ok(CDS_DRIVE_NOT_READY) => DriveStatus::NotReady,
            // CDS_DISC_OK, or CDS_NO_INFO from drives that can't tell: reading the disc decides.
            Ok(_) | Err(_) => DriveStatus::DiscPresent,
        }
    }

    pub(super) fn read_toc(path: &Path) -> io::Result<Toc> {
        let file = open(path)?;
        let mut header = TocHeader::default();
        ioctl(&file, CDROMREADTOCHDR, &raw mut header)?;
        let entry = |track: u8| -> io::Result<TocEntry> {
            let mut entry = TocEntry {
                track,
                format: CDROM_LBA,
                ..TocEntry::default()
            };
            ioctl(&file, CDROMREADTOCENTRY, &raw mut entry)?;
            Ok(entry)
        };
        let tracks = (header.first..=header.last)
            .map(|number| {
                let entry = entry(number)?;
                Ok(TocTrack {
                    number,
                    start: u32::try_from(entry.lba).map_err(io::Error::other)?,
                    audio: (entry.adr_ctrl >> 4) & CDROM_DATA_TRACK == 0,
                })
            })
            .collect::<io::Result<Vec<_>>>()?;
        let leadout = u32::try_from(entry(CDROM_LEADOUT)?.lba).map_err(io::Error::other)?;
        if tracks.is_empty() {
            return Err(io::Error::other("The disc has no tracks"));
        }
        Ok(Toc { tracks, leadout })
    }

    pub(super) fn eject(path: &Path) -> io::Result<()> {
        ioctl(&open(path)?, CDROMEJECT, 0).map(drop)
    }

    pub(super) fn close_tray(path: &Path) -> io::Result<()> {
        ioctl(&open(path)?, CDROMCLOSETRAY, 0).map(drop)
    }
}

#[cfg(not(target_os = "linux"))]
mod device {
    use super::{Drive, DriveStatus, Toc};
    use std::{io, path::Path};

    pub(super) fn devices() -> Vec<Drive> {
        Vec::new()
    }
    pub(super) fn status(_: &Path) -> DriveStatus {
        DriveStatus::NoDisc
    }
    pub(super) fn read_toc(_: &Path) -> io::Result<Toc> {
        Err(io::ErrorKind::Unsupported.into())
    }
    pub(super) fn eject(_: &Path) -> io::Result<()> {
        Err(io::ErrorKind::Unsupported.into())
    }
    pub(super) fn close_tray(_: &Path) -> io::Result<()> {
        Err(io::ErrorKind::Unsupported.into())
    }
}

pub(super) mod simulated {
    use super::{AudioTrack, Toc, TocTrack, LEAD_IN, SECTOR_BYTES, SIMULATED_SPEED};
    use std::{
        f32::consts::TAU,
        io,
        sync::atomic::AtomicBool,
        time::{Duration, Instant},
    };

    pub(super) static TRAY_OPEN: AtomicBool = AtomicBool::new(false);

    /// Radiohead, In Rainbows (US CD), MusicBrainz disc ID 7G2ACzTNyOeu30btlmsgkicwVRE-.
    const OFFSETS: [u32; 10] = [
        150, 18158, 36369, 55675, 79691, 96904, 106592, 128325, 153017, 171729,
    ];
    const SECTORS: u32 = 192_945;

    pub(super) fn toc() -> Toc {
        Toc {
            tracks: (1..)
                .zip(OFFSETS)
                .map(|(number, offset)| TocTrack {
                    number,
                    start: offset - LEAD_IN,
                    audio: true,
                })
                .collect(),
            leadout: SECTORS - LEAD_IN,
        }
    }

    /// A quiet tone, a different note per track, read at a real drive's pace.
    pub(crate) struct Tone {
        frequency: f32,
        frame: u64,
        frames: u64,
        started: Instant,
    }

    impl Tone {
        pub(super) fn new(track: AudioTrack) -> Self {
            Self {
                frequency: 220.0 * 2f32.powf(f32::from(track.number) / 12.0),
                frame: 0,
                frames: track.bytes() / 4,
                started: Instant::now(),
            }
        }

        pub(super) fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            let frames = ((buffer.len() / 4) as u64).min(self.frames - self.frame);
            for (index, chunk) in buffer
                .as_chunks_mut::<4>()
                .0
                .iter_mut()
                .take(frames as usize)
                .enumerate()
            {
                let time = (self.frame + index as u64) as f32 / 44_100.0;
                let sample = ((TAU * self.frequency * time).sin() * 3_000.0) as i16;
                let bytes = sample.to_le_bytes();
                chunk.copy_from_slice(&[bytes[0], bytes[1], bytes[0], bytes[1]]);
            }
            self.frame += frames;
            let due = Duration::from_secs_f64(
                (self.frame * 4) as f64 / (SECTOR_BYTES * 75 * SIMULATED_SPEED) as f64,
            );
            if let Some(wait) = due.checked_sub(self.started.elapsed()) {
                std::thread::sleep(wait);
            }
            Ok(frames as usize * 4)
        }
    }
}
