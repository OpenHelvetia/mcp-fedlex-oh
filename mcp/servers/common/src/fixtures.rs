//! The semantic fixture store two domain servers share: a recorded
//! answer lives under the first eight bytes of its key's SHA-256, and
//! `INDEX.txt` says which key that file holds and on which day it was
//! recorded.
//!
//! Lifted out of `mcp/servers/fedlex/src/backend.rs` at BX, unchanged:
//! the file names and the index format are byte-identical, which is
//! what let the fedlex fixtures stay untouched by the move.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use anyhow::Result;

/// The file name a fixture key is stored under — the first eight bytes
/// of its SHA-256, hex. Public because a test that has to BUILD a case
/// the recorded corpus does not carry must write its double under the
/// name the backend will look for.
pub fn fixture_file_name(key: &str) -> String {
    key_file(Path::new(""), key)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// The path a key's answer is stored at, under `dir`.
pub fn key_file(dir: &Path, key: &str) -> PathBuf {
    use sha2::Digest as _;
    let digest = sha2::Sha256::digest(key.as_bytes());
    let mut name = String::new();
    for b in digest.iter().take(8) {
        let _ = write!(name, "{b:02x}");
    }
    dir.join(format!("{name}.json"))
}

/// The KEY of one `INDEX.txt` line — the parser, not the writer.
///
/// The format is `<file> <key> <recorded>`; the file name and the date
/// are single tokens, so the key is everything between them —
/// unambiguous however many spaces it carries, and a search query
/// carries several (`search_law:Bundesgesetz über die politischen
/// Rechte:10`). `None` for a line that is not of that shape.
pub fn key_of(line: &str) -> Option<&str> {
    let (_, rest) = line.split_once(' ')?;
    let (key, _) = rest.rsplit_once(' ')?;
    (!key.is_empty()).then_some(key)
}

/// WRITES one `file key recorded-date` line to the human-auditable
/// `INDEX.txt`, the date being the DAY THE RECORDING WAS MADE (BV A′:
/// a fixture without its moment is an undated claim).
///
/// A key recorded again keeps its file and gets the new date —
/// replacing its line rather than appending beside it. Lines beginning
/// with `#` are notes and are carried through untouched.
///
/// # Errors
///
/// The index could not be read or written.
pub fn index_line(dir: &Path, file: &Path, key: &str) -> Result<()> {
    let index = dir.join("INDEX.txt");
    let existing = std::fs::read_to_string(&index).unwrap_or_default();
    let name = file
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let today = now_rfc3339().split('T').next().unwrap_or("").to_string();
    let line = format!("{name} {key} {today}");
    let mut out = String::with_capacity(existing.len() + line.len() + 1);
    let mut replaced = false;
    for old in existing.lines() {
        // A NOTE is never a candidate, however many tokens it carries
        // (BY″): `key_of` reads «everything between the first and the
        // last token», so a note whose middle run happened to equal
        // the key being written was silently replaced by the new line
        // — the one thing the line above this function promises never
        // happens.
        if old.starts_with('#') {
            out.push_str(old);
            out.push('\n');
            continue;
        }
        // A KEY MAY CARRY SPACES — `search_law:Bundesgesetz über die
        // politischen Rechte:10` is one (BY point 0). The line is
        // `<file> <key> <recorded>`, so the key is everything between
        // the FIRST space and the LAST one; splitting on every space
        // compared «search_law:Bundesgesetz» to the whole key, never
        // matched, and appended a second line for the same key on
        // every re-recording.
        let same_key = key_of(old).is_some_and(|k| k == key);
        if same_key {
            out.push_str(&line);
            out.push('\n');
            replaced = true;
        } else {
            out.push_str(old);
            out.push('\n');
        }
    }
    if !replaced {
        out.push_str(&line);
        out.push('\n');
    }
    std::fs::write(&index, out)?;
    Ok(())
}

/// The moment of a real request, RFC 3339 in UTC — read from the clock
/// HERE, at the network call, and nowhere else in a domain library:
/// the domain never reads a clock, but the transaction time of a
/// retrieval is the retrieval's own fact.
pub fn now_rfc3339() -> String {
    time::OffsetDateTime::now_utc()
        .replace_nanosecond(0)
        .unwrap_or_else(|_| time::OffsetDateTime::now_utc())
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The file name is a function of the KEY and of nothing else —
    /// the property the fedlex fixtures rely on across this move.
    #[test]
    fn a_key_names_its_file_and_nothing_else_does() {
        assert_eq!(
            fixture_file_name("resolve_sr:candidates:832.10"),
            "6e01faeb23575d73.json",
            "a key recorded before this crate existed must still find its file"
        );
        assert_eq!(
            fixture_file_name(
                "manifestation:https://fedlex.data.admin.ch/eli/cc/2006/355/20231101:de"
            ),
            "5adfd70a311cc955.json"
        );
        assert_ne!(fixture_file_name("a"), fixture_file_name("b"));
    }

    /// A `#` note is carried through untouched — even one whose middle
    /// token run IS the key being written (BY″: such a note was
    /// silently replaced, while the writer's own doc line promised
    /// notes survive; BY‴: the note this test sowed did not actually
    /// collide, so the clause could be deleted with three suites
    /// green).
    #[test]
    fn a_note_is_never_mistaken_for_the_key_being_written() {
        let dir = std::env::temp_dir().join(format!("oh-common-notes-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        let key = "resolve_sr:candidates:832.10";
        // A note whose MIDDLE TOKEN RUN is the key, byte for byte:
        // `key_of` reads «everything between the first token and the
        // last», so this line decomposes to exactly the key being
        // written. That is the only note that could ever be mistaken
        // for it — and the one the first version of this test did not
        // write (it wrote «# recorded <key> by hand», which decomposes
        // to «recorded <key> by» and collides with nothing).
        let colliding = format!("# {key} 2020-01-01");
        assert_eq!(
            key_of(&colliding),
            Some(key),
            "the note must really look like the line the writer is about to write"
        );
        std::fs::write(
            dir.join("INDEX.txt"),
            format!("{colliding}\nold.json other:key 2020-01-01\n"),
        )
        .expect("seed");
        let file = key_file(&dir, key);
        std::fs::write(&file, "{}").expect("fixture");
        index_line(&dir, &file, key).expect("indexed");
        let index = std::fs::read_to_string(dir.join("INDEX.txt")).expect("index");
        assert!(
            index.contains(&colliding),
            "the note survived, though it decomposes to the very key being written: {index}"
        );
        assert!(
            index
                .lines()
                .any(|l| !l.starts_with('#') && key_of(l) == Some(key)),
            "and the key got its own line: {index}"
        );
        assert!(
            index.contains("old.json other:key"),
            "and so did the other key"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_key_may_carry_spaces_and_is_still_replaced_not_repeated() {
        let dir = std::env::temp_dir().join(format!("oh-common-spaced-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        let key = "search_law:Bundesgesetz über die politischen Rechte:10";
        let file = key_file(&dir, key);
        std::fs::write(&file, "{}").expect("fixture");
        index_line(&dir, &file, key).expect("indexed");
        index_line(&dir, &file, key).expect("re-indexed");
        let index = std::fs::read_to_string(dir.join("INDEX.txt")).expect("index");
        assert_eq!(
            index.lines().filter(|l| l.contains(key)).count(),
            1,
            "one key, one line: {index}"
        );
        assert_eq!(
            index.lines().next().and_then(key_of),
            Some(key),
            "and the key reads back whole, spaces and all: {index}"
        );
        assert_eq!(key_of("a.json k 2026-08-30"), Some("k"));
        assert_eq!(key_of("nonsense"), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A key recorded again keeps its file and gets the new date; the
    /// notes above the lines survive.
    #[test]
    fn the_index_replaces_a_key_and_keeps_the_notes() {
        let dir = std::env::temp_dir().join(format!("oh-common-index-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        std::fs::write(
            dir.join("INDEX.txt"),
            "# a note\nold.json other:key 2020-01-01\n",
        )
        .expect("seed");
        let file = key_file(&dir, "tool:arg");
        index_line(&dir, &file, "tool:arg").expect("writes");
        index_line(&dir, &file, "tool:arg").expect("writes again");
        let text = std::fs::read_to_string(dir.join("INDEX.txt")).expect("read");
        assert!(text.starts_with("# a note\n"), "{text}");
        assert!(text.contains("old.json other:key 2020-01-01"), "{text}");
        assert_eq!(
            text.matches("tool:arg").count(),
            1,
            "a key recorded twice has ONE line: {text}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
