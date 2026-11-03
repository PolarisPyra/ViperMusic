//! MusicBrainz: a disc's ID, the releases it belongs to, and their front covers from the Cover Art
//! Archive.
use super::drive::{Toc, LEAD_IN};
use base64::Engine;
use serde::Deserialize;
use sha1::{Digest, Sha1};
use std::{fmt::Write, sync::OnceLock, time::Duration};

static HTTP_CLIENT: OnceLock<Result<reqwest::blocking::Client, String>> = OnceLock::new();

/// One release the disc belongs to, with the tracks of the disc's medium.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Release {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) artist: String,
    /// The release date's year, when MusicBrainz has one.
    pub(crate) year: Option<u32>,
    pub(crate) country: Option<String>,
    /// Which disc of the release this is, from 1.
    pub(crate) disc_number: u32,
    pub(crate) disc_count: u32,
    pub(crate) tracks: Vec<ReleaseTrack>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ReleaseTrack {
    pub(crate) title: String,
    pub(crate) artist: String,
    pub(crate) recording_id: Option<String>,
}

impl Release {
    /// How the release is told apart from others with the same disc.
    pub(crate) fn label(&self) -> String {
        let mut label = format!("{} — {}", self.title, self.artist);
        for detail in [self.year.map(|year| year.to_string()), self.country.clone()]
            .into_iter()
            .flatten()
        {
            let _ = write!(label, " · {detail}");
        }
        if self.disc_count > 1 {
            let _ = write!(label, " · disc {} of {}", self.disc_number, self.disc_count);
        }
        label
    }
}

/// The disc's MusicBrainz disc ID: a SHA-1 of its first and last audio tracks, lead-out, and
/// track offsets, in MusicBrainz's URL-safe Base64.
pub(crate) fn disc_id(toc: &Toc) -> String {
    let tracks = toc.audio_tracks();
    let (first, last) = match (tracks.first(), tracks.last()) {
        (Some(first), Some(last)) => (first.number, last.number),
        _ => (1, 0),
    };
    let mut text = format!("{first:02X}{last:02X}{:08X}", toc.audio_leadout() + LEAD_IN);
    for index in 1..=99u8 {
        let offset = tracks
            .iter()
            .find(|track| track.number == index)
            .map_or(0, |track| track.start + LEAD_IN);
        let _ = write!(text, "{offset:08X}");
    }
    base64::engine::general_purpose::STANDARD
        .encode(Sha1::digest(text.as_bytes()))
        .chars()
        .map(|character| match character {
            '+' => '.',
            '/' => '_',
            '=' => '-',
            other => other,
        })
        .collect()
}

/// The page where a disc MusicBrainz doesn't know yet is added to a release, prefilled with it.
pub(crate) fn submission_url(toc: &Toc) -> String {
    format!(
        "https://musicbrainz.org/cdtoc/attach?id={}&tracks={}&toc={}",
        disc_id(toc),
        toc.audio_tracks().len(),
        toc_parameter(toc).replace(' ', "+")
    )
}

/// The table of contents in the form MusicBrainz's `toc` parameter takes, for releases that have
/// the same tracks but not this exact disc ID.
fn toc_parameter(toc: &Toc) -> String {
    let tracks = toc.audio_tracks();
    let mut parameter = format!(
        "{} {} {}",
        tracks.first().map_or(1, |track| track.number),
        tracks.last().map_or(0, |track| track.number),
        toc.audio_leadout() + LEAD_IN
    );
    for track in &tracks {
        let _ = write!(parameter, " {}", track.start + LEAD_IN);
    }
    parameter
}

fn client() -> Result<reqwest::blocking::Client, String> {
    HTTP_CLIENT
        .get_or_init(|| {
            reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(20))
                // MusicBrainz asks every client to name itself and a way to reach its authors.
                .user_agent(concat!(
                    "ViperMusic/",
                    env!("CARGO_PKG_VERSION"),
                    " ( https://github.com/PolarisPyra/ViperMusic )"
                ))
                .build()
                .map_err(|error| format!("Could not prepare the MusicBrainz request: {error}"))
        })
        .clone()
}

/// The releases that have this disc, best match first. Empty when MusicBrainz doesn't know it.
pub(crate) fn lookup(toc: &Toc) -> Result<Vec<Release>, String> {
    let id = disc_id(toc);
    let response = client()?
        .get(format!("https://musicbrainz.org/ws/2/discid/{id}"))
        .query(&[
            ("toc", toc_parameter(toc).as_str()),
            ("inc", "artist-credits recordings"),
            ("fmt", "json"),
        ])
        .send()
        .map_err(|error| network_error("MusicBrainz", error))?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(Vec::new());
    }
    let response: DiscResponse = response
        .error_for_status()
        .map_err(|error| format!("MusicBrainz returned an error: {}", error.without_url()))?
        .json()
        .map_err(|error| {
            format!(
                "Could not read the MusicBrainz response: {}",
                error.without_url()
            )
        })?;
    let track_count = toc.audio_tracks().len();
    let mut releases: Vec<(bool, Release)> = response
        .releases
        .into_iter()
        .filter_map(|release| {
            let disc_count = release.media.len() as u32;
            let (exact, position, medium) = release
                .media
                .iter()
                .enumerate()
                .find(|(_, medium)| medium.discs.iter().any(|disc| disc.id == id))
                .map(|(index, medium)| (true, index, medium))
                .or_else(|| {
                    release
                        .media
                        .iter()
                        .enumerate()
                        .find(|(_, medium)| medium.tracks.len() == track_count)
                        .map(|(index, medium)| (false, index, medium))
                })?;
            let artist = credit(&release.artist_credit);
            Some((
                exact,
                Release {
                    tracks: medium
                        .tracks
                        .iter()
                        .map(|track| ReleaseTrack {
                            title: track.title.clone(),
                            artist: if track.artist_credit.is_empty() {
                                artist.clone()
                            } else {
                                credit(&track.artist_credit)
                            },
                            recording_id: track
                                .recording
                                .as_ref()
                                .map(|recording| recording.id.clone()),
                        })
                        .collect(),
                    id: release.id,
                    title: release.title,
                    artist,
                    year: release
                        .date
                        .as_deref()
                        .and_then(|date| date.get(..4))
                        .and_then(|year| year.parse().ok()),
                    country: release.country,
                    disc_number: medium.position.unwrap_or(position as u32 + 1),
                    disc_count,
                },
            ))
        })
        .collect();
    // Releases with this exact disc come before those that only match its track count.
    releases.sort_by_key(|(exact, _)| !exact);
    Ok(releases.into_iter().map(|(_, release)| release).collect())
}

/// The release's front cover, about 500 px wide. `None` when the archive has none.
pub(crate) fn front_cover(release_id: &str) -> Result<Option<Vec<u8>>, String> {
    let response = client()?
        .get(format!(
            "https://coverartarchive.org/release/{release_id}/front-500"
        ))
        .send()
        .map_err(|error| network_error("the Cover Art Archive", error))?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    let bytes = response
        .error_for_status()
        .map_err(|error| {
            format!(
                "The Cover Art Archive returned an error: {}",
                error.without_url()
            )
        })?
        .bytes()
        .map_err(|error| format!("Could not download the cover: {}", error.without_url()))?;
    Ok(Some(bytes.to_vec()))
}

/// Why a service could not be reached, in words for the user rather than the HTTP client's.
fn network_error(service: &str, error: reqwest::Error) -> String {
    if error.is_timeout() {
        format!("{service} took too long to answer. Try again in a moment.")
    } else if error.is_connect() {
        format!("Could not reach {service}. Check your internet connection.")
    } else {
        format!(
            "Could not reach {service}: {error}",
            error = error.without_url()
        )
    }
}

fn credit(credits: &[Credit]) -> String {
    credits
        .iter()
        .map(|credit| format!("{}{}", credit.name, credit.joinphrase))
        .collect()
}

#[derive(Deserialize)]
struct DiscResponse {
    #[serde(default)]
    releases: Vec<MbRelease>,
}

#[derive(Deserialize)]
struct MbRelease {
    id: String,
    title: String,
    date: Option<String>,
    country: Option<String>,
    #[serde(rename = "artist-credit", default)]
    artist_credit: Vec<Credit>,
    #[serde(default)]
    media: Vec<Medium>,
}

#[derive(Deserialize)]
struct Credit {
    name: String,
    #[serde(default)]
    joinphrase: String,
}

#[derive(Deserialize)]
struct Medium {
    position: Option<u32>,
    #[serde(default)]
    discs: Vec<Disc>,
    #[serde(default)]
    tracks: Vec<MbTrack>,
}

#[derive(Deserialize)]
struct Disc {
    id: String,
}

#[derive(Deserialize)]
struct MbTrack {
    title: String,
    #[serde(rename = "artist-credit", default)]
    artist_credit: Vec<Credit>,
    recording: Option<Recording>,
}

#[derive(Deserialize)]
struct Recording {
    id: String,
}
