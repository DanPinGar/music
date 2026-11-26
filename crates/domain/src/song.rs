//! The song a video plays: its title, artist and year, guessed from a video's title.

/// What is known of the song a video plays.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SongDetails {
    pub title: String,
    pub artist: Option<String>,
    /// The year the song (or the album it is on) was published.
    pub year: Option<i32>,
}

/// Words that mark a bracketed part or a `|` segment of a title as describing the video, not
/// the song: "(Piano Tutorial)", "[Stride Piano Synthesia]", "| Sheet Music".
const NOISE: &[&str] = &[
    "4k",
    "advanced",
    "arr",
    "arrangement",
    "audio",
    "cover",
    "easy",
    "hd",
    "instrumental",
    "karaoke",
    "lesson",
    "lyrics",
    "midi",
    "official",
    "piano",
    "remaster",
    "remastered",
    "sheet",
    "sheets",
    "synthesia",
    "tutorial",
    "version",
    "video",
];

/// Years a title may name: four digits from the first printed music to this century.
const YEARS: std::ops::RangeInclusive<i32> = 1600..=2099;

/// The song in a title such as YouTube's piano videos have: "Artist - Song 1936 (Piano
/// Tutorial)", "Song (Piano) - Genre // ARTIST", "Artist - Song | Synthesia". What only
/// describes the video is dropped, a year standing alone becomes the year, and an artist
/// written in capitals is written normally. A title it cannot read is the song's title, as
/// it is.
pub fn guess_song(text: &str) -> SongDetails {
    let original = squeeze(text);
    let mut year = None;
    let text = without_noise_groups(&original, &mut year);

    // "Song - Genre // ARTIST": some channels name the artist last, after two slashes.
    let (main, named_artist) = match text.rsplit_once(" // ") {
        Some((main, artist)) => (main.to_owned(), first_segment(artist)),
        None => (text.clone(), None),
    };
    // "Artist - Song | Piano Tutorial | Sheet Music": the song comes first.
    let mut main = first_segment(&main).unwrap_or_default();
    match take_year(&main) {
        Some((found, rest)) => {
            year = year.or(Some(found));
            main = rest;
        }
        None => year = year.or_else(|| take_year(&text).map(|(found, _)| found)),
    }

    let parts = split_dashes(&main);
    let (title, artist) = match (named_artist, parts.as_slice()) {
        (Some(artist), [title, ..]) => (tidy(title), Some(tidy(&artist))),
        (None, [artist, rest @ ..]) if !rest.is_empty() => {
            (tidy(&rest.join(" - ")), Some(tidy(artist)))
        }
        _ => (tidy(&main), None),
    };
    let artist = artist.filter(|artist| !artist.is_empty());
    if title.is_empty() {
        return SongDetails {
            title: original,
            artist: None,
            year: None,
        };
    }
    SongDetails {
        title,
        artist: artist.map(|artist| normal_case(&artist)),
        year,
    }
}

/// `text` without the bracketed parts that describe the video; a part that is only a year,
/// "(1936)", gives the year.
fn without_noise_groups(text: &str, year: &mut Option<i32>) -> String {
    let mut kept = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find(['(', '[', '{']) {
        let close_char = match rest.as_bytes()[open] {
            b'(' => ')',
            b'[' => ']',
            _ => '}',
        };
        let Some(length) = rest[open + 1..].find(close_char) else {
            break;
        };
        let inside = &rest[open + 1..open + 1 + length];
        kept.push_str(&rest[..open]);
        if let Some(found) = only_year(inside) {
            *year = year.or(Some(found));
        } else if !is_noise(inside) {
            kept.push_str(&rest[open..open + 2 + length]);
        }
        rest = &rest[open + 2 + length..];
    }
    kept.push_str(rest);
    squeeze(&kept)
}

/// The first `|` segment of `text`, unless it only describes the video.
fn first_segment(text: &str) -> Option<String> {
    let first = text.split(" | ").next().unwrap_or(text).trim();
    (!first.is_empty() && !is_noise(first)).then(|| first.to_owned())
}

fn is_noise(text: &str) -> bool {
    text.split(|c: char| !c.is_alphanumeric())
        .any(|word| NOISE.contains(&word.to_lowercase().as_str()))
}

fn only_year(text: &str) -> Option<i32> {
    let text = text.trim();
    (text.len() == 4 && text.bytes().all(|b| b.is_ascii_digit()))
        .then(|| text.parse().ok())
        .flatten()
        .filter(|year| YEARS.contains(year))
}

/// The last year standing alone in `text` ("Passwonky 1936"), and `text` without it.
fn take_year(text: &str) -> Option<(i32, String)> {
    let bytes = text.as_bytes();
    let alone = |index: usize| {
        text[index..]
            .chars()
            .next()
            .is_none_or(|c| !c.is_alphanumeric())
    };
    let mut found = None;
    let mut start = 0;
    while start < bytes.len() {
        if !bytes[start].is_ascii_digit() {
            start += 1;
            continue;
        }
        let end = start
            + bytes[start..]
                .iter()
                .take_while(|b| b.is_ascii_digit())
                .count();
        let before_alone = text[..start]
            .chars()
            .next_back()
            .is_none_or(|c| !c.is_alphanumeric());
        if end - start == 4
            && before_alone
            && alone(end)
            && let Some(year) = only_year(&text[start..end])
        {
            found = Some((year, start, end));
        }
        start = end;
    }
    let (year, start, end) = found?;
    Some((
        year,
        squeeze(&format!("{} {}", &text[..start], &text[end..])),
    ))
}

/// "Artist - Song - Live": the parts between spaced dashes (hyphen, en or em dash).
fn split_dashes(text: &str) -> Vec<String> {
    text.replace(" – ", " - ")
        .replace(" — ", " - ")
        .split(" - ")
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Without the punctuation left at the ends, nor a colon before a bracket: "Dinah: (Take 1)"
/// is "Dinah (Take 1)".
fn tidy(text: &str) -> String {
    let text = squeeze(text);
    let mut tidied = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == ':' && chars.clone().find(|c| *c != ' ').is_some_and(|c| c == '(') {
            continue;
        }
        tidied.push(c);
    }
    squeeze(&tidied)
        .trim_matches(|c: char| c.is_whitespace() || "-–—:;,|/".contains(c))
        .to_owned()
}

/// "CARLOS GARDEL" as "Carlos Gardel"; a single word in capitals ("ABBA") stays as it is.
fn normal_case(name: &str) -> String {
    let shouting = name.contains(' ')
        && name.chars().any(char::is_alphabetic)
        && !name.chars().any(char::is_lowercase);
    if !shouting {
        return name.to_owned();
    }
    name.split(' ')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first
                    .to_uppercase()
                    .chain(chars.flat_map(char::to_lowercase))
                    .collect(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Trimmed, with runs of whitespace as one space.
fn squeeze(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn song(title: &str, artist: Option<&str>, year: Option<i32>) -> SongDetails {
        SongDetails {
            title: title.into(),
            artist: artist.map(str::to_owned),
            year,
        }
    }

    #[test]
    fn reads_the_titles_of_piano_videos() {
        for (text, expected) in [
            (
                r#"Thomas "Fats" Waller - Passwonky 1936 (Fast Stride Piano Synthesia)"#,
                song("Passwonky", Some(r#"Thomas "Fats" Waller"#), Some(1936)),
            ),
            (
                "Por una Cabeza (Piano) - Tango // CARLOS GARDEL",
                song("Por una Cabeza", Some("Carlos Gardel"), None),
            ),
            (
                "Thelonious Monk - Dinah: (Take 1) 1965 [Stride Piano Synthesia]",
                song("Dinah (Take 1)", Some("Thelonious Monk"), Some(1965)),
            ),
            (
                "Ludovico Einaudi - Nuvole Bianche (Piano Tutorial)",
                song("Nuvole Bianche", Some("Ludovico Einaudi"), None),
            ),
            (
                "Interstellar – Main Theme | Piano Tutorial | Sheet Music",
                song("Main Theme", Some("Interstellar"), None),
            ),
            (
                "Debussy - Clair de Lune (1905)",
                song("Clair de Lune", Some("Debussy"), Some(1905)),
            ),
            (
                "Chopin - Nocturne Op. 9 No. 2",
                song("Nocturne Op. 9 No. 2", Some("Chopin"), None),
            ),
            (
                "Bach - Cello Suite No. 1, BWV 1007",
                song("Cello Suite No. 1, BWV 1007", Some("Bach"), None),
            ),
            (
                "ABBA - Dancing Queen",
                song("Dancing Queen", Some("ABBA"), None),
            ),
            ("  Clair   de Lune ", song("Clair de Lune", None, None)),
        ] {
            assert_eq!(guess_song(text), expected, "{text}");
        }
    }

    #[test]
    fn ignores_years_that_describe_the_video() {
        assert_eq!(
            guess_song("Queen - Bohemian Rhapsody (Remastered 2011)"),
            song("Bohemian Rhapsody", Some("Queen"), None)
        );
        assert_eq!(
            guess_song("Dave Brubeck - Take Five 1959 | Piano Cover 2024"),
            song("Take Five", Some("Dave Brubeck"), Some(1959))
        );
    }

    #[test]
    fn keeps_a_title_made_only_of_noise() {
        assert_eq!(
            guess_song("[Piano Tutorial]"),
            song("[Piano Tutorial]", None, None)
        );
    }
}
