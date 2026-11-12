//! The release manifest a published copy polls: the version it names, the asset to fetch and the checks an offer must pass before it is made.

use serde::Deserialize;

const SHA256_HEX_LENGTH: usize = 64;

const HTTPS_SCHEME: &str = "https://";

const LOOPBACK_PREFIXES: [&str; 2] = ["http://127.0.0.1", "http://localhost"];

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

/// One file a release publishes, and its digest.
#[derive(Deserialize)]
struct Asset {
    url: String,
    sha256: String,
}

/// The manifest as the repository hosts it: the portable exe at the top level
/// (what every released copy has always read), the MSI beside it.
#[derive(Deserialize)]
struct Manifest {
    version: String,
    #[serde(flatten)]
    executable: Asset,
    installer: Option<Asset>,
}

/// Which of a release's files this copy installs from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Package {
    /// The portable exe, swapped in for the running one.
    Executable,
    /// The MSI, for a copy that MSI installed.
    Installer,
}

impl Package {
    pub(super) fn file_name(self, version: Version) -> String {
        match self {
            Self::Executable => format!("PADE-{version}.exe"),
            Self::Installer => format!("PADE-{version}-setup.msi"),
        }
    }
}

/// A release newer than the running one: the file to fetch and its digest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Offer {
    pub(super) version: Version,
    pub(super) url: String,
    pub(super) sha256: String,
}

/// The offer a manifest makes to a copy running `current` that installs from
/// `package` - none unless it names a newer version and, for that file, a full
/// SHA-256 and an https address (or this machine's, for a test's stand-in
/// server).
pub(super) fn offer_from(manifest: &str, current: Version, package: Package) -> Option<Offer> {
    let manifest: Manifest = serde_json::from_str(manifest).ok()?;
    let version = Version::parse(&manifest.version)?;
    let asset = match package {
        Package::Executable => manifest.executable,
        Package::Installer => manifest.installer?,
    };
    let has_digest = asset.sha256.len() == SHA256_HEX_LENGTH
        && asset
            .sha256
            .chars()
            .all(|character| character.is_ascii_hexdigit());
    let is_trusted_address = asset.url.starts_with(HTTPS_SCHEME)
        || LOOPBACK_PREFIXES
            .iter()
            .any(|prefix| asset.url.starts_with(prefix));
    let is_offered = version > current && has_digest && is_trusted_address;
    is_offered.then(|| Offer {
        version,
        url: asset.url,
        sha256: asset.sha256.to_ascii_lowercase(),
    })
}
