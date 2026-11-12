use super::manifest::{offer_from, Offer, Version};
use super::Updater;

const RUNNING: Version = Version {
    major: 0,
    minor: 0,
    patch: 2,
};
const RELEASE_URL: &str = "https://github.com/avi12/pade/releases/tag/v0.0.3";

fn manifest(version: &str, release_url: &str) -> String {
    format!(r#"{{"version":"{version}","releaseUrl":"{release_url}"}}"#)
}

#[test]
fn versions_order_the_way_releases_do() {
    let parsed = |text| Version::parse(text).expect("a version");
    assert!(parsed("0.0.10") > parsed("0.0.9"));
    assert!(parsed("0.1.0") > parsed("0.0.99"));
    assert_eq!(parsed("v1.2.3"), parsed("1.2.3"));
    assert!(Version::parse("1.2").is_none());
    assert!(Version::parse("1.2.3.4").is_none());
    assert_eq!(parsed("1.2.3").to_string(), "1.2.3");
}

#[test]
fn only_an_unpackaged_published_copy_is_told_to_install_the_package() {
    assert_eq!(Updater::of(true, true), Updater::Windows);
    assert_eq!(Updater::of(true, false), Updater::Windows);
    assert_eq!(Updater::of(false, true), Updater::Unpackaged);
    assert_eq!(Updater::of(false, false), Updater::None);
}

#[test]
fn a_newer_release_is_offered_with_its_page() {
    assert_eq!(
        offer_from(&manifest("0.0.3", RELEASE_URL), RUNNING),
        Some(Offer {
            version: Version::parse("0.0.3").expect("a version"),
            release_url: RELEASE_URL.to_string(),
        })
    );
}

#[test]
fn the_same_or_an_older_release_is_not_offered() {
    assert!(offer_from(&manifest("0.0.2", RELEASE_URL), RUNNING).is_none());
    assert!(offer_from(&manifest("0.0.1", RELEASE_URL), RUNNING).is_none());
}

#[test]
fn nothing_is_offered_without_an_https_page_or_from_a_malformed_manifest() {
    assert!(offer_from(&manifest("0.0.3", "http://example.com/release"), RUNNING).is_none());
    assert!(offer_from(&manifest("0.0.3", "javascript:alert(1)"), RUNNING).is_none());
    assert!(offer_from(&manifest("soon", RELEASE_URL), RUNNING).is_none());
    assert!(offer_from("not json", RUNNING).is_none());
    assert!(offer_from(r#"{"version":"0.0.3","url":"x","sha256":"y"}"#, RUNNING).is_none());
}
