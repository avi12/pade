//! The release manifest an unpackaged copy polls: the version it names, the release page to send the user to and the checks a notice must pass.

use serde::Deserialize;

const HTTPS_SCHEME: &str = "https://";

/// A release version, `major.minor.patch`, ordered the way releases are.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Version {
    pub(super) major: u64,
    pub(super) minor: u64,
    pub(super) patch: u64,
}

impl Version {
    pub(super) fn parse(text: &str) -> Option<Self> {
        let mut parts = text.trim().trim_start_matches('v').split('.');
        let version = Self {
            major: parts.next()?.parse().ok()?,
            minor: parts.next()?.parse().ok()?,
            patch: parts.next()?.parse().ok()?,
        };
        parts.next().is_none().then_some(version)
    }

    pub(super) fn running() -> Option<Self> {
        Self::parse(env!("CARGO_PKG_VERSION"))
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// The manifest as the repository hosts it.
#[derive(Deserialize)]
struct Manifest {
    version: String,
    #[serde(rename = "releaseUrl")]
    release_url: String,
}

/// A release newer than the running one, and the page to install it from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Offer {
    pub(super) version: Version,
    pub(super) release_url: String,
}

/// The offer a manifest makes to a copy running `current` - none unless it
/// names a newer version and an https release page.
pub(super) fn offer_from(manifest: &str, current: Version) -> Option<Offer> {
    let manifest: Manifest = serde_json::from_str(manifest).ok()?;
    let version = Version::parse(&manifest.version)?;
    let is_offered = version > current && manifest.release_url.starts_with(HTTPS_SCHEME);
    is_offered.then_some(Offer {
        version,
        release_url: manifest.release_url,
    })
}
