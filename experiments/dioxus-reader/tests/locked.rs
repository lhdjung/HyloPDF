//! A document behind a password: asking for one, refusing a wrong one, and
//! what happens to the reader who declines.
//!
//! `ui.askForPassword` in the app, which was the largest thing left on
//! `PROGRESS.md`'s list of what this port did not have — an encrypted document
//! opened as an error, in a reader whose whole premise is that it opens what
//! you give it.
//!
//! The fixture is written here rather than found: [`fixture::locked_pdf`] is
//! the PDF standard security handler at revision 2, RC4 at 40 bits, which is
//! forty lines of MD5 and RC4 and no dependency. See its own comment for why
//! the weakest variant in the spec is the right one to test against.

use dioxus_reader::fixture;

/// MD5 against the RFC's own vectors, because everything below stands on it:
/// a key derivation that is quietly wrong makes a fixture no reader can open,
/// and the failure would look exactly like the feature not working.
#[test]
fn the_digest_the_key_is_derived_with_is_md5() {
    for (input, expected) in [
        ("", "d41d8cd98f00b204e9800998ecf8427e"),
        ("a", "0cc175b9c0f1b6a831c399e269772661"),
        ("abc", "900150983cd24fb0d6963f7d28e17f72"),
        ("message digest", "f96b697d7cb7938d525a2f31aaf161d0"),
        ("abcdefghijklmnopqrstuvwxyz", "c3fcd3d76192e4007dfb496cca67e13b"),
        (
            "12345678901234567890123456789012345678901234567890123456789012345678901234567890",
            "57edf4a22be3c955ac49da2e2107b67a",
        ),
    ] {
        assert_eq!(
            fixture::digest_for_test(input.as_bytes()),
            expected,
            "md5 of {input:?}",
        );
    }
}

/// And the document it makes is one no reader opens without the password.
#[test]
fn the_fixture_is_really_locked() {
    assert!(
        dioxus_reader::render::open(&fixture::locked_pdf()).is_err(),
        "opened a locked document with no password",
    );
}
