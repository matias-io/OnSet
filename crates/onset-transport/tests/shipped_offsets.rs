//! The offsets files that ship in `offsets/` parse and carry what the live reader needs.
use onset_transport::memory::offsets::Offsets;

fn shipped() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../offsets")
}

#[test]
fn every_shipped_version_has_a_signature() {
    for version in ["7.2.18", "7.2.19"] {
        let o = Offsets::for_version(&shipped(), version).expect(version);
        let sig = o.signature.expect("signature");
        assert!(sig.anchors.len() >= 3, "{version}");
    }
}

#[test]
fn rekordbox_7_2_19_knows_the_master_deck() {
    let o = Offsets::for_version(&shipped(), "7.2.19").expect("7.2.19");
    let flag = o
        .signature
        .and_then(|s| s.master_flag)
        .expect("master flag");
    assert_eq!((flag.offset, flag.mask), (-396, 1));
}

/// The static routes to the MASTER array found on 2026-10-22 did not survive a rekordbox
/// restart (the first named deck 4 with deck 1 lit), so 7.2.19 reads MASTER from the deck
/// objects' flag until a location that holds across launches is found.
#[test]
fn rekordbox_7_2_19_ships_no_master_routes() {
    let o = Offsets::for_version(&shipped(), "7.2.19").expect("7.2.19");
    assert!(o.signature.and_then(|s| s.master_chains).is_none());
}
