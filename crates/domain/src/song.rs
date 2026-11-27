//! The song a video plays: its title and artist, guessed from a video's title.

/// What a video's title tells of the song it plays. Never its year: only the user gives that.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SongDetails {
    pub title: String,
    pub artist: Option<String>,
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
/// describes the video is dropped, and so is a year after the song's name (the song's year
/// is the user's to give); an artist written in capitals is written normally. A title it
/// cannot read is the song's title, as it is.
pub fn guess_song(text: &str) -> SongDetails {
    let original = squeeze(text);
    let text = without_noise_groups(&original);

    // "Song - Genre // ARTIST": some channels name the artist last, after two slashes.
    let (main, named_artist) = match text.rsplit_once(" // ") {
        Some((main, artist)) => (main.to_owned(), first_segment(artist)),
        None => (text.clone(), None),
    };
    // "Artist - Song | Piano Tutorial | Sheet Music": the song comes first.
    let main = first_segment(&main).unwrap_or_default();

    let parts = split_dashes(&main);
    let (title, artist) = match (named_artist, parts.as_slice()) {
        (Some(artist), [title, ..]) => (title.clone(), Some(artist)),
        (None, [artist, rest @ ..]) if !rest.is_empty() => (rest.join(" - "), Some(artist.clone())),
        _ => (main, None),
    };
    let title = tidy(&without_year(&title));
    let artist = artist
        .map(|artist| tidy(&artist))
        .filter(|artist| !artist.is_empty());
    if title.is_empty() {
        return SongDetails {
            title: original,
            artist: None,
        };
    }
    SongDetails {
        title,
        artist: artist.map(|artist| normal_case(&artist)),
    }
}

/// `text` without the bracketed parts that describe the video, nor those that are only a
/// year, "(1936)".
fn without_noise_groups(text: &str) -> String {
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
        if only_year(inside).is_none() && !is_noise(inside) {
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

/// `text` without a year at its end ("Passwonky 1936"), unless the year is all there is
/// ("1999") or is not at the end ("1812 Overture"): then it is the song's name.
fn without_year(text: &str) -> String {
    let text = text.trim_end();
    let split = text.len().saturating_sub(4);
    if !text.is_char_boundary(split) {
        return text.to_owned();
    }
    let (rest, year) = text.split_at(split);
    let alone = rest
        .chars()
        .next_back()
        .is_some_and(|c| !c.is_alphanumeric());
    if alone && only_year(year).is_some() && !tidy(rest).is_empty() {
        rest.to_owned()
    } else {
        text.to_owned()
    }
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

    fn song(title: &str, artist: Option<&str>) -> SongDetails {
        SongDetails {
            title: title.into(),
            artist: artist.map(str::to_owned),
        }
    }

    #[test]
    fn reads_the_titles_of_piano_videos() {
        for (text, expected) in [
            (
                r#"Thomas "Fats" Waller - Passwonky 1936 (Fast Stride Piano Synthesia)"#,
                song("Passwonky", Some(r#"Thomas "Fats" Waller"#)),
            ),
            (
                "Por una Cabeza (Piano) - Tango // CARLOS GARDEL",
                song("Por una Cabeza", Some("Carlos Gardel")),
            ),
            (
                "Thelonious Monk - Dinah: (Take 1) 1965 [Stride Piano Synthesia]",
                song("Dinah (Take 1)", Some("Thelonious Monk")),
            ),
            (
                "Ludovico Einaudi - Nuvole Bianche (Piano Tutorial)",
                song("Nuvole Bianche", Some("Ludovico Einaudi")),
            ),
            (
                "Interstellar – Main Theme | Piano Tutorial | Sheet Music",
                song("Main Theme", Some("Interstellar")),
            ),
            (
                "Chopin - Nocturne Op. 9 No. 2",
                song("Nocturne Op. 9 No. 2", Some("Chopin")),
            ),
            (
                "Bach - Cello Suite No. 1, BWV 1007",
                song("Cello Suite No. 1, BWV 1007", Some("Bach")),
            ),
            ("ABBA - Dancing Queen", song("Dancing Queen", Some("ABBA"))),
            ("  Clair   de Lune ", song("Clair de Lune", None)),
        ] {
            assert_eq!(guess_song(text), expected, "{text}");
        }
    }

    #[test]
    fn leaves_the_year_out_of_the_song() {
        for (text, expected) in [
            (
                "Debussy - Clair de Lune (1905)",
                song("Clair de Lune", Some("Debussy")),
            ),
            (
                "Queen - Bohemian Rhapsody (Remastered 2011)",
                song("Bohemian Rhapsody", Some("Queen")),
            ),
            (
                "Dave Brubeck - Take Five 1959 | Piano Cover 2024",
                song("Take Five", Some("Dave Brubeck")),
            ),
            ("Chopin - Nocturne - 1832", song("Nocturne", Some("Chopin"))),
            // A year that is the song's name, or starts it, stays.
            ("Prince - 1999", song("1999", Some("Prince"))),
            (
                "Tchaikovsky - 1812 Overture",
                song("1812 Overture", Some("Tchaikovsky")),
            ),
        ] {
            assert_eq!(guess_song(text), expected, "{text}");
        }
    }

    #[test]
    fn keeps_a_title_made_only_of_noise() {
        assert_eq!(
            guess_song("[Piano Tutorial]"),
            song("[Piano Tutorial]", None)
        );
    }
}
