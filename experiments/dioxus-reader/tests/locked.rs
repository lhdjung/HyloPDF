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

use dioxus_reader::fixture::{self, LOCKED_PASSWORD};

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

/// The renderer's own answer, under the interface: locked is a *kind* of
/// refusal and not a sentence, which is what lets everything above it tell a
/// question from a failure.
#[test]
fn pdfium_says_locked_rather_than_broken() {
    let path = fixture::locked_pdf();
    assert_eq!(
        dioxus_reader::render::open(&path).err(),
        Some(dioxus_reader::render::Refusal::Locked),
        "a locked document with no password",
    );
    let wrong = dioxus_reader::render::open_with(&path, Some("not the password"));
    assert_eq!(
        wrong.err(),
        Some(dioxus_reader::render::Refusal::Locked),
        "and a wrong one, which pdfium reports identically",
    );
    let opened = dioxus_reader::render::open_with(&path, Some(LOCKED_PASSWORD))
        .unwrap_or_else(|err| panic!("the password did not open it: {err}"));
    assert_eq!(opened.pages(), 3);
    assert!(opened.encrypted(), "and it knows it came in through a lock");
}
