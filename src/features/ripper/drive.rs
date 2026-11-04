//! Optical drives: finding them, their tray and disc status, reading a disc's table of contents,
//! and streaming a track's audio.
//!
//! Linux talks to `/dev/srN` and reads audio through cdparanoia, Windows to the drive letter's
//! device, and macOS asks `drutil` for its drives and reads the disc's raw device.
//!
//! Set `VIPER_SIMULATED_CD=1` to add a simulated drive holding the table of contents of a real
//! album (Radiohead's In Rainbows), so the ripper can be developed and tried without a drive or a
//! disc. MusicBrainz knows the disc, and its tracks play quiet tones.
#[cfg(target_os = "linux")]
use std::process::{Child, ChildStdout, Command, Stdio};
use std::{
    io::{self, Read},
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
/// Sectors asked for in one direct read, under the 64 KiB most drives and connections take.
#[cfg(not(target_os = "linux"))]
const SECTORS_PER_READ: u32 = 26;
/// Tries at a direct read before the rip fails, since a scratch can need a few.
#[cfg(not(target_os = "linux"))]
const READ_ATTEMPTS: u32 = 5;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Drive {
    /// A real drive: `/dev/srN` on Linux, `\\.\D:` on Windows, and its number in `drutil list` on
    /// macOS.
    Device { id: String, name: String },
    /// The development drive enabled by `VIPER_SIMULATED_CD`.
    Simulated,
}

/// What the drive reports about its tray and disc.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DriveStatus {
    TrayOpen,
    NoDisc,
    /// The drive is spinning up or reading a newly inserted disc. Windows can't tell this from
    /// an empty drive.
    #[cfg_attr(windows, allow(dead_code))]
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
            Self::Device { id, .. } => device::status(id),
            Self::Simulated if simulated::TRAY_OPEN.load(Ordering::Relaxed) => {
                DriveStatus::TrayOpen
            }
            Self::Simulated => DriveStatus::DiscPresent,
        }
    }

    pub(crate) fn read_toc(&self) -> io::Result<Toc> {
        match self {
            Self::Device { id, .. } => device::read_toc(id),
            Self::Simulated => Ok(simulated::toc()),
        }
    }

    pub(crate) fn eject(&self) -> io::Result<()> {
        match self {
            Self::Device { id, .. } => device::eject(id),
            Self::Simulated => {
                simulated::TRAY_OPEN.store(true, Ordering::Relaxed);
                Ok(())
            }
        }
    }

    pub(crate) fn close_tray(&self) -> io::Result<()> {
        match self {
            Self::Device { id, .. } => device::close_tray(id),
            Self::Simulated => {
                simulated::TRAY_OPEN.store(false, Ordering::Relaxed);
                Ok(())
            }
        }
    }

    /// Start reading one track as raw 16-bit little-endian stereo PCM. On Linux, real drives read
    /// through cdparanoia, which re-reads and verifies until each sector is right or it gives up;
    /// on Windows and macOS they read directly, trying each read again when it fails.
    pub(crate) fn open_track(&self, track: AudioTrack) -> io::Result<TrackReader> {
        match self {
            #[cfg(target_os = "linux")]
            Self::Device { id, .. } => {
                let mut child = Command::new("cdparanoia")
                    .arg("--force-cdrom-device")
                    .arg(id)
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
            #[cfg(not(target_os = "linux"))]
            Self::Device { id, .. } => Ok(TrackReader::Direct(DirectReader {
                disc: device::open_audio(id)?,
                next: track.start,
                end: track.start + track.sectors,
                buffer: Vec::new(),
                position: 0,
            })),
            Self::Simulated => Ok(TrackReader::Simulated(simulated::Tone::new(track))),
        }
    }

    /// Whether this drive reads audio through cdparanoia, which only Linux uses.
    pub(crate) fn reads_with_paranoia(&self) -> bool {
        cfg!(target_os = "linux") && matches!(self, Self::Device { .. })
    }
}

/// Whether cdparanoia, which reads audio from real drives on Linux, is installed.
pub(crate) fn paranoia_installed() -> bool {
    std::env::var_os("PATH").is_some_and(|paths| {
        std::env::split_paths(&paths).any(|dir| dir.join("cdparanoia").is_file())
    })
}

/// A track being read from the disc.
pub(crate) enum TrackReader {
    #[cfg(target_os = "linux")]
    Paranoia {
        child: Child,
        stdout: ChildStdout,
    },
    #[cfg(not(target_os = "linux"))]
    Direct(DirectReader),
    Simulated(simulated::Tone),
}

impl Read for TrackReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        match self {
            #[cfg(target_os = "linux")]
            Self::Paranoia { stdout, .. } => stdout.read(buffer),
            #[cfg(not(target_os = "linux"))]
            Self::Direct(reader) => reader.read(buffer),
            Self::Simulated(tone) => tone.read(buffer),
        }
    }
}

impl TrackReader {
    /// Wait for the read to end, and fail when cdparanoia reported an error.
    pub(crate) fn finish(mut self) -> io::Result<()> {
        match &mut self {
            #[cfg(target_os = "linux")]
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
            #[cfg(not(target_os = "linux"))]
            Self::Direct(_) => Ok(()),
            Self::Simulated(_) => Ok(()),
        }
    }
}

#[cfg(target_os = "linux")]
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

/// A track read straight from the drive, a few sectors at a time.
#[cfg(not(target_os = "linux"))]
pub(crate) struct DirectReader {
    disc: device::Disc,
    /// The next sector to read, as a logical block address.
    next: u32,
    end: u32,
    buffer: Vec<u8>,
    /// How much of `buffer` has been handed out.
    position: usize,
}

#[cfg(not(target_os = "linux"))]
impl DirectReader {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if self.position == self.buffer.len() {
            let sectors = SECTORS_PER_READ.min(self.end - self.next);
            if sectors == 0 {
                return Ok(0);
            }
            self.buffer
                .resize(sectors as usize * SECTOR_BYTES as usize, 0);
            let mut attempt = 1;
            while let Err(error) = self.disc.read(self.next, &mut self.buffer) {
                if attempt == READ_ATTEMPTS {
                    return Err(io::Error::new(
                        error.kind(),
                        format!("Could not read sector {}: {error}", self.next),
                    ));
                }
                attempt += 1;
            }
            self.next += sectors;
            self.position = 0;
        }
        let count = out.len().min(self.buffer.len() - self.position);
        out[..count].copy_from_slice(&self.buffer[self.position..][..count]);
        self.position += count;
        Ok(count)
    }
}

#[cfg(target_os = "linux")]
mod device {
    use super::{Drive, DriveStatus, Toc, TocTrack};
    use std::{
        fs::{self, File, OpenOptions},
        io,
        os::{fd::AsRawFd, unix::fs::OpenOptionsExt},
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
                    let id = format!("/dev/{device}");
                    Drive::Device {
                        name: if label.is_empty() { id.clone() } else { label },
                        id,
                    }
                })
            })
            .collect();
        drives.sort_by(|a, b| match (a, b) {
            (Drive::Device { id: a, .. }, Drive::Device { id: b, .. }) => a.cmp(b),
            _ => std::cmp::Ordering::Equal,
        });
        drives
    }

    /// Open without waiting for a disc, so status checks work with the tray open.
    fn open(id: &str) -> io::Result<File> {
        OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(id)
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

    pub(super) fn status(id: &str) -> DriveStatus {
        let file = match open(id) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::PermissionDenied => {
                return DriveStatus::Inaccessible(format!(
                    "Viper Music is not allowed to open {id}. Add yourself to the group that owns \
                     it (often \"optical\"), then sign in again."
                ));
            }
            Err(error) => {
                return DriveStatus::Inaccessible(format!("Could not open {id}: {error}"))
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

    pub(super) fn read_toc(id: &str) -> io::Result<Toc> {
        let file = open(id)?;
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

    pub(super) fn eject(id: &str) -> io::Result<()> {
        ioctl(&open(id)?, CDROMEJECT, 0).map(drop)
    }

    pub(super) fn close_tray(id: &str) -> io::Result<()> {
        ioctl(&open(id)?, CDROMCLOSETRAY, 0).map(drop)
    }
}

#[cfg(windows)]
mod device {
    use super::{Drive, DriveStatus, Toc, TocTrack, LEAD_IN};
    use std::{
        ffi::c_void,
        fs::{File, OpenOptions},
        io,
        os::windows::{fs::OpenOptionsExt, io::AsRawHandle},
        ptr,
    };
    use windows_sys::Win32::{
        Devices::Cdrom::{
            CDDA, CDROM_TOC, IOCTL_CDROM_RAW_READ, IOCTL_CDROM_READ_TOC, RAW_READ_INFO, TRACK_DATA,
        },
        Foundation::ERROR_NOT_READY,
        Storage::FileSystem::{GetDriveTypeW, GetLogicalDrives, FILE_SHARE_READ, FILE_SHARE_WRITE},
        System::{
            Ioctl::{
                PropertyStandardQuery, StorageDeviceProperty, IOCTL_STORAGE_CHECK_VERIFY2,
                IOCTL_STORAGE_EJECT_MEDIA, IOCTL_STORAGE_LOAD_MEDIA, IOCTL_STORAGE_QUERY_PROPERTY,
                STORAGE_PROPERTY_QUERY,
            },
            WindowsProgramming::DRIVE_CDROM,
            IO::DeviceIoControl,
        },
    };

    /// Raw reads place a sector by its offset in 2048-byte data sectors.
    const DATA_SECTOR_BYTES: i64 = 2048;
    const LEADOUT: u8 = 0xAA;
    /// In a track's control bits, which `TRACK_DATA` keeps in the low four bits.
    const DATA_TRACK: u8 = 0x04;

    pub(super) fn devices() -> Vec<Drive> {
        // SAFETY: takes no arguments.
        let letters = unsafe { GetLogicalDrives() };
        ('A'..='Z')
            .zip(0..)
            .filter(|(_, bit)| letters & (1 << bit) != 0)
            .filter(|(letter, _)| {
                let root: Vec<u16> = format!("{letter}:\\").encode_utf16().chain([0]).collect();
                // SAFETY: `root` is a NUL-terminated wide string.
                unsafe { GetDriveTypeW(root.as_ptr()) == DRIVE_CDROM }
            })
            .map(|(letter, _)| {
                let id = format!(r"\\.\{letter}:");
                Drive::Device {
                    name: match product(&id) {
                        Some(product) => format!("{product} ({letter}:)"),
                        None => format!("Drive {letter}:"),
                    },
                    id,
                }
            })
            .collect()
    }

    fn open(id: &str) -> io::Result<File> {
        OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .open(id)
    }

    /// Send `request` to the drive and return how many bytes it wrote to `output`.
    ///
    /// # Safety
    /// `input` and `output` must point to `input_len` and `output_len` bytes laid out as the
    /// request expects, or be null with a length of 0.
    unsafe fn control(
        file: &File,
        request: u32,
        input: *const c_void,
        input_len: usize,
        output: *mut c_void,
        output_len: usize,
    ) -> io::Result<usize> {
        let mut written = 0;
        // SAFETY: the caller vouches for the buffers, and `file` is open.
        let succeeded = unsafe {
            DeviceIoControl(
                file.as_raw_handle(),
                request,
                input,
                input_len as u32,
                output,
                output_len as u32,
                &mut written,
                ptr::null_mut(),
            )
        };
        if succeeded == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(written as usize)
        }
    }

    /// The drive's vendor and model, as it reports them.
    fn product(id: &str) -> Option<String> {
        let file = open(id).ok()?;
        let query = STORAGE_PROPERTY_QUERY {
            PropertyId: StorageDeviceProperty,
            QueryType: PropertyStandardQuery,
            AdditionalParameters: [0],
        };
        let mut descriptor = [0_u8; 1024];
        // SAFETY: the input is a STORAGE_PROPERTY_QUERY, and the output a byte buffer.
        let written = unsafe {
            control(
                &file,
                IOCTL_STORAGE_QUERY_PROPERTY,
                (&raw const query).cast(),
                size_of_val(&query),
                descriptor.as_mut_ptr().cast(),
                descriptor.len(),
            )
        }
        .ok()?;
        let descriptor = &descriptor[..written];
        // A STORAGE_DEVICE_DESCRIPTOR keeps the offsets of its NUL-terminated vendor and product
        // IDs at bytes 12 and 16, with 0 for one it doesn't have.
        let text = |at: usize| {
            let offset = u32::from_le_bytes(descriptor.get(at..at + 4)?.try_into().ok()?) as usize;
            let bytes = descriptor.get(offset..).filter(|_| offset != 0)?;
            let end = bytes
                .iter()
                .position(|&byte| byte == 0)
                .unwrap_or(bytes.len());
            Some(String::from_utf8_lossy(&bytes[..end]).trim().to_owned())
        };
        let label = [text(12), text(16)]
            .into_iter()
            .flatten()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        (!label.is_empty()).then_some(label)
    }

    pub(super) fn status(id: &str) -> DriveStatus {
        let file = match open(id) {
            Ok(file) => file,
            Err(error) => {
                let letter = id.trim_start_matches(r"\\.\");
                return DriveStatus::Inaccessible(format!(
                    "Could not open drive {letter} ({error})"
                ));
            }
        };
        // SAFETY: the request takes no buffers.
        match unsafe {
            control(
                &file,
                IOCTL_STORAGE_CHECK_VERIFY2,
                ptr::null(),
                0,
                ptr::null_mut(),
                0,
            )
        } {
            // Windows doesn't tell an open tray from an empty one, or a disc still spinning up.
            Err(error) if error.raw_os_error() == Some(ERROR_NOT_READY as i32) => {
                DriveStatus::NoDisc
            }
            // A disc, or a drive that can't tell: reading the disc decides.
            Ok(_) | Err(_) => DriveStatus::DiscPresent,
        }
    }

    pub(super) fn read_toc(id: &str) -> io::Result<Toc> {
        let file = open(id)?;
        let mut toc = CDROM_TOC::default();
        // SAFETY: the output is a CDROM_TOC, as the request expects.
        unsafe {
            control(
                &file,
                IOCTL_CDROM_READ_TOC,
                ptr::null(),
                0,
                (&raw mut toc).cast(),
                size_of::<CDROM_TOC>(),
            )
        }?;
        let malformed = || io::Error::other("The disc's table of contents is malformed");
        // The entries run from the first track to the lead-out, at minutes, seconds and frames
        // from the start of the lead-in.
        let count = usize::from(
            toc.LastTrack
                .checked_sub(toc.FirstTrack)
                .ok_or_else(malformed)?,
        );
        let (leadout, tracks) = toc
            .TrackData
            .get(..=count + 1)
            .and_then(<[TRACK_DATA]>::split_last)
            .filter(|(leadout, _)| leadout.TrackNumber == LEADOUT)
            .ok_or_else(malformed)?;
        let start = |entry: &TRACK_DATA| {
            let [_, minutes, seconds, frames] = entry.Address.map(u32::from);
            ((minutes * 60 + seconds) * 75 + frames)
                .checked_sub(LEAD_IN)
                .ok_or_else(malformed)
        };
        let tracks = tracks
            .iter()
            .map(|entry| {
                Ok(TocTrack {
                    number: entry.TrackNumber,
                    start: start(entry)?,
                    audio: entry._bitfield & DATA_TRACK == 0,
                })
            })
            .collect::<io::Result<Vec<_>>>()?;
        Ok(Toc {
            tracks,
            leadout: start(leadout)?,
        })
    }

    pub(super) fn eject(id: &str) -> io::Result<()> {
        // SAFETY: the request takes no buffers.
        unsafe {
            control(
                &open(id)?,
                IOCTL_STORAGE_EJECT_MEDIA,
                ptr::null(),
                0,
                ptr::null_mut(),
                0,
            )
        }
        .map(drop)
    }

    pub(super) fn close_tray(id: &str) -> io::Result<()> {
        // SAFETY: the request takes no buffers.
        unsafe {
            control(
                &open(id)?,
                IOCTL_STORAGE_LOAD_MEDIA,
                ptr::null(),
                0,
                ptr::null_mut(),
                0,
            )
        }
        .map(drop)
    }

    /// A drive opened to read audio.
    pub(super) struct Disc(File);

    pub(super) fn open_audio(id: &str) -> io::Result<Disc> {
        open(id).map(Disc)
    }

    impl Disc {
        /// Fill `buffer` with whole audio sectors from `start` on.
        pub(super) fn read(&self, start: u32, buffer: &mut [u8]) -> io::Result<()> {
            let info = RAW_READ_INFO {
                DiskOffset: i64::from(start) * DATA_SECTOR_BYTES,
                SectorCount: (buffer.len() / super::SECTOR_BYTES as usize) as u32,
                TrackMode: CDDA,
            };
            // SAFETY: the input is a RAW_READ_INFO, and the output holds the sectors it asks for.
            let read = unsafe {
                control(
                    &self.0,
                    IOCTL_CDROM_RAW_READ,
                    (&raw const info).cast(),
                    size_of::<RAW_READ_INFO>(),
                    buffer.as_mut_ptr().cast(),
                    buffer.len(),
                )
            }?;
            if read == buffer.len() {
                Ok(())
            } else {
                Err(io::Error::other(
                    "The drive read less than it was asked for",
                ))
            }
        }
    }
}

#[cfg(target_os = "macos")]
mod device {
    use super::{drutil, Drive, DriveStatus, Toc, SECTOR_BYTES};
    use std::{
        fs::File,
        io,
        os::fd::AsRawFd,
        process::{Command, Stdio},
    };

    // From <IOKit/storage/IOCDMediaBSDClient.h> and <IOKit/storage/IOCDTypes.h>.
    const DKIOCCDREAD: libc::c_ulong = 0xC020_6460;
    const DKIOCCDREADTOC: libc::c_ulong = 0xC018_6464;
    const CD_TOC_FORMAT_TOC: u8 = 0x02;
    const CD_SECTOR_AREA_USER: u8 = 0x10;
    const CD_SECTOR_TYPE_CDDA: u8 = 0x01;

    /// `dk_cd_read_toc_t`.
    #[repr(C)]
    struct ReadToc {
        format: u8,
        format_as_time: u8,
        reserved0: [u8; 5],
        session: u8,
        reserved1: [u8; 6],
        buffer_length: u16,
        buffer: *mut libc::c_void,
    }

    /// `dk_cd_read_t`.
    #[repr(C)]
    struct ReadSectors {
        /// In bytes of the sector area read, 2352 per audio sector.
        offset: u64,
        sector_area: u8,
        sector_type: u8,
        reserved0: [u8; 10],
        buffer_length: u32,
        buffer: *mut libc::c_void,
    }

    // The request numbers encode these sizes.
    const _: () = assert!(size_of::<ReadToc>() == 24 && size_of::<ReadSectors>() == 32);

    /// Run `drutil` for one drive, or for all of them without `id`.
    fn run(id: Option<&str>, verb: &[&str]) -> io::Result<String> {
        let mut command = Command::new("drutil");
        if let Some(id) = id {
            command.args(["-drive", id]);
        }
        let output = command
            .args(verb)
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()?;
        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).into_owned())
        } else {
            Err(io::Error::other(format!(
                "drutil {} failed ({})",
                verb.join(" "),
                output.status
            )))
        }
    }

    fn ioctl<T>(file: &File, request: libc::c_ulong, argument: *mut T) -> io::Result<()> {
        // SAFETY: each caller passes the struct the request expects, with a valid buffer.
        if unsafe { libc::ioctl(file.as_raw_fd(), request, argument) } < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    pub(super) fn devices() -> Vec<Drive> {
        run(None, &["list"])
            .map(|list| drutil::drives(&list))
            .unwrap_or_default()
    }

    /// Open the raw device of the disc in the drive.
    fn open_disc(id: &str) -> io::Result<File> {
        let device = drutil::disc(&run(Some(id), &["status"])?)
            .map_err(|_| io::Error::other("There is no disc in the drive"))?;
        File::open(device)
    }

    pub(super) fn status(id: &str) -> DriveStatus {
        let device = match run(Some(id), &["status"]).map(|status| drutil::disc(&status)) {
            Ok(Ok(device)) => device,
            Ok(Err(status)) => return status,
            Err(error) => {
                return DriveStatus::Inaccessible(format!("Could not ask drutil: {error}"))
            }
        };
        match File::open(&device) {
            Err(error) if error.kind() == io::ErrorKind::PermissionDenied => {
                DriveStatus::Inaccessible(format!("Viper Music is not allowed to read {device}."))
            }
            // Reading the disc decides.
            Ok(_) | Err(_) => DriveStatus::DiscPresent,
        }
    }

    pub(super) fn read_toc(id: &str) -> io::Result<Toc> {
        let file = open_disc(id)?;
        let mut buffer = vec![0_u8; 2048];
        let mut request = ReadToc {
            format: CD_TOC_FORMAT_TOC,
            format_as_time: 1,
            reserved0: [0; 5],
            session: 0,
            reserved1: [0; 6],
            buffer_length: buffer.len() as u16,
            buffer: buffer.as_mut_ptr().cast(),
        };
        ioctl(&file, DKIOCCDREADTOC, &raw mut request)?;
        drutil::raw_toc(&buffer)
    }

    pub(super) fn eject(id: &str) -> io::Result<()> {
        run(Some(id), &["tray", "eject"]).map(drop)
    }

    pub(super) fn close_tray(id: &str) -> io::Result<()> {
        run(Some(id), &["tray", "close"]).map(drop)
    }

    /// The raw device of a disc, opened to read audio.
    pub(super) struct Disc(File);

    pub(super) fn open_audio(id: &str) -> io::Result<Disc> {
        open_disc(id).map(Disc)
    }

    impl Disc {
        /// Fill `buffer` with whole audio sectors from `start` on.
        pub(super) fn read(&self, start: u32, buffer: &mut [u8]) -> io::Result<()> {
            let mut request = ReadSectors {
                offset: u64::from(start) * SECTOR_BYTES,
                sector_area: CD_SECTOR_AREA_USER,
                sector_type: CD_SECTOR_TYPE_CDDA,
                reserved0: [0; 10],
                buffer_length: buffer.len() as u32,
                buffer: buffer.as_mut_ptr().cast(),
            };
            ioctl(&self.0, DKIOCCDREAD, &raw mut request)?;
            if request.buffer_length as usize == buffer.len() {
                Ok(())
            } else {
                Err(io::Error::other(
                    "The drive read less than it was asked for",
                ))
            }
        }
    }
}

/// Reading what `drutil` prints and the raw table of contents macOS returns, kept apart from the
/// system calls so they can be tested anywhere.
#[cfg(any(target_os = "macos", test))]
mod drutil {
    use super::{Drive, DriveStatus, Toc, TocTrack, LEAD_IN};
    use std::io;

    const LEADOUT: u8 = 0xA2;
    const DATA_TRACK: u8 = 0x04;

    /// The drives in `drutil list`, named by the text under its Vendor and Product columns.
    pub(super) fn drives(list: &str) -> Vec<Drive> {
        let mut lines = list.lines();
        let Some(header) = lines.find(|line| line.contains("Vendor")) else {
            return Vec::new();
        };
        let columns = header.find("Vendor").zip(header.find("Rev"));
        lines
            .filter_map(|line| {
                let id = line
                    .split_whitespace()
                    .next()
                    .filter(|id| id.bytes().all(|byte| byte.is_ascii_digit()))?;
                let name = columns
                    .and_then(|(start, end)| line.get(start..end.min(line.len())))
                    .map(|name| name.split_whitespace().collect::<Vec<_>>().join(" "))
                    .filter(|name| !name.is_empty())
                    .unwrap_or_else(|| format!("Drive {id}"));
                Some(Drive::Device {
                    id: id.to_owned(),
                    name,
                })
            })
            .collect()
    }

    /// The raw device of the disc `drutil status` names, or the drive's status without one.
    pub(super) fn disc(status: &str) -> Result<String, DriveStatus> {
        if status.contains("No Media") {
            return Err(DriveStatus::NoDisc);
        }
        status
            .split_whitespace()
            .skip_while(|word| *word != "Name:")
            .nth(1)
            .and_then(|name| name.strip_prefix("/dev/disk"))
            .map(|number| format!("/dev/rdisk{number}"))
            // A disc macOS hasn't finished recognizing has no device yet.
            .ok_or(DriveStatus::NotReady)
    }

    /// A table of contents from the raw form (a `CDTOC`): a big-endian length and the first and
    /// last sessions, then 11-byte descriptors whose PMIN, PSEC and PFRAME place each track
    /// (points 1 to 99) and each session's lead-out (point A2) from the start of the lead-in.
    pub(super) fn raw_toc(bytes: &[u8]) -> io::Result<Toc> {
        let malformed = || io::Error::other("The disc's table of contents is malformed");
        let length = bytes
            .first_chunk::<2>()
            .map(|length| usize::from(u16::from_be_bytes(*length)) + 2)
            .ok_or_else(malformed)?;
        let (descriptors, _) = bytes
            .get(4..length)
            .ok_or_else(malformed)?
            .as_chunks::<11>();
        let mut tracks = Vec::new();
        let mut leadout = None;
        for &[session, adr_control, _, point, _, _, _, _, minutes, seconds, frames] in descriptors {
            // Other ADR modes carry catalogue numbers and the like.
            if adr_control >> 4 != 1 {
                continue;
            }
            let start = ((u32::from(minutes) * 60 + u32::from(seconds)) * 75 + u32::from(frames))
                .checked_sub(LEAD_IN)
                .ok_or_else(malformed)?;
            match point {
                1..=99 => tracks.push(TocTrack {
                    number: point,
                    start,
                    audio: adr_control & DATA_TRACK == 0,
                }),
                // The disc's lead-out is the last session's.
                LEADOUT => leadout = leadout.max(Some((session, start))),
                _ => {}
            }
        }
        tracks.sort_by_key(|track| track.number);
        if tracks.is_empty() {
            return Err(io::Error::other("The disc has no tracks"));
        }
        let (_, leadout) = leadout.ok_or_else(malformed)?;
        Ok(Toc { tracks, leadout })
    }
}

#[cfg(not(any(target_os = "linux", windows, target_os = "macos")))]
mod device {
    use super::{Drive, DriveStatus, Toc};
    use std::io;

    pub(super) fn devices() -> Vec<Drive> {
        Vec::new()
    }
    pub(super) fn status(_: &str) -> DriveStatus {
        DriveStatus::NoDisc
    }
    pub(super) fn read_toc(_: &str) -> io::Result<Toc> {
        Err(io::ErrorKind::Unsupported.into())
    }
    pub(super) fn eject(_: &str) -> io::Result<()> {
        Err(io::ErrorKind::Unsupported.into())
    }
    pub(super) fn close_tray(_: &str) -> io::Result<()> {
        Err(io::ErrorKind::Unsupported.into())
    }

    pub(super) enum Disc {}

    pub(super) fn open_audio(_: &str) -> io::Result<Disc> {
        Err(io::ErrorKind::Unsupported.into())
    }

    impl Disc {
        pub(super) fn read(&self, _: u32, _: &mut [u8]) -> io::Result<()> {
            match *self {}
        }
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

#[cfg(test)]
mod tests {
    use super::{drutil, simulated, Drive, DriveStatus, LEAD_IN};

    #[test]
    fn reads_a_raw_toc() {
        let toc = simulated::toc();
        let descriptor = |session: u8, adr_control: u8, point: u8, lba: u32| {
            let msf = lba + LEAD_IN;
            let [minutes, seconds, frames] =
                [msf / 75 / 60, msf / 75 % 60, msf % 75].map(|part| part as u8);
            [
                session,
                adr_control,
                0,
                point,
                0,
                0,
                0,
                0,
                minutes,
                seconds,
                frames,
            ]
        };
        let mut descriptors = vec![
            // The first and last track numbers, and a catalogue number, which carry no address.
            [1, 0x10, 0, 0xA0, 0, 0, 0, 0, 1, 0, 0],
            [1, 0x10, 0, 0xA1, 0, 0, 0, 0, 10, 0, 0],
            [1, 0x50, 0, 0xB0, 0, 0, 0, 0, 0, 0, 0],
            descriptor(1, 0x10, 0xA2, toc.leadout),
        ];
        // Listed last to first, to check the order the tracks come back in.
        descriptors.extend(
            toc.tracks
                .iter()
                .rev()
                .map(|track| descriptor(1, 0x10, track.number, track.start)),
        );
        let body = descriptors.concat();
        let mut bytes = ((body.len() + 2) as u16).to_be_bytes().to_vec();
        bytes.extend([1, 1]);
        bytes.extend(body);
        // The driver's buffer is larger than the table.
        bytes.resize(2048, 0);
        assert_eq!(drutil::raw_toc(&bytes).unwrap(), toc);
    }

    #[test]
    fn reads_drutil_list() {
        let list = "   Vendor   Product           Rev   Bus       SupportLevel\n\
                    1  MATSHITA DVD-R   UJ-8A8    HA13  ATAPI     Apple Shipping\n\
                    2  HL-DT-ST DVDRW  GX40N      RQ01  USB       Unsupported\n";
        assert_eq!(
            drutil::drives(list),
            [
                Drive::Device {
                    id: "1".to_owned(),
                    name: "MATSHITA DVD-R UJ-8A8".to_owned()
                },
                Drive::Device {
                    id: "2".to_owned(),
                    name: "HL-DT-ST DVDRW GX40N".to_owned()
                },
            ]
        );
        assert!(drutil::drives("No burning devices connected.\n").is_empty());
    }

    #[test]
    fn reads_drutil_status() {
        let header = " Vendor   Product           Rev \n MATSHITA DVD-R   UJ-8A8    HA13\n\n";
        assert_eq!(
            drutil::disc(&format!(
                "{header}           Type: CD-ROM               Name: /dev/disk4\n       Sessions: 1                  Tracks: 10\n"
            )),
            Ok("/dev/rdisk4".to_owned())
        );
        assert_eq!(
            drutil::disc(&format!("{header}           Type: No Media Inserted\n")),
            Err(DriveStatus::NoDisc)
        );
        assert_eq!(
            drutil::disc(&format!("{header}           Type: CD-ROM\n")),
            Err(DriveStatus::NotReady)
        );
    }
}
