//! Title templates such as `[%artist% - ]%title%` (spec §4.8). Parsing never fails: what is
//! not valid syntax is taken literally.

use crate::entry::Tags;

/// The template a new config starts with.
pub const DEFAULT: &str = "[%artist% - ]%title%";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Template(Vec<Part>);

#[derive(Clone, Debug, PartialEq, Eq)]
enum Part {
    Text(String),
    Field(Field),
    /// `[...]`: shown only when every field directly inside has a value. A section nested
    /// inside decides for itself.
    Section(Vec<Part>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Field {
    Artist,
    Title,
    Album,
    Year,
    Track,
    Filename,
    /// Expands to nothing.
    Unknown,
}

impl Default for Template {
    fn default() -> Self {
        Template::parse(DEFAULT)
    }
}

impl Template {
    pub fn parse(text: &str) -> Template {
        let chars: Vec<char> = text.chars().collect();
        let mut pos = 0;
        Template(parse(&chars, &mut pos, false).0)
    }

    /// Expands the template; `filename` is the value of `%filename%`.
    pub fn render(&self, tags: &Tags, filename: &str) -> String {
        let mut out = String::new();
        render(&self.0, tags, filename, &mut out);
        out
    }
}

/// Parses up to the end, or inside a section up to its `]`. Returns the parts and whether
/// the section was closed.
fn parse(chars: &[char], pos: &mut usize, in_section: bool) -> (Vec<Part>, bool) {
    let mut parts = Vec::new();
    let mut text = String::new();
    while let Some(&c) = chars.get(*pos) {
        let doubled = chars.get(*pos + 1) == Some(&c);
        match c {
            '%' | '[' | ']' if doubled => {
                text.push(c);
                *pos += 2;
            }
            '%' => match field_name(&chars[*pos + 1..]) {
                Some(name) => {
                    flush(&mut text, &mut parts);
                    parts.push(Part::Field(field(&name)));
                    *pos += name.chars().count() + 2;
                }
                None => {
                    text.push('%');
                    *pos += 1;
                }
            },
            '[' => {
                *pos += 1;
                flush(&mut text, &mut parts);
                let (inner, closed) = parse(chars, pos, true);
                if closed {
                    parts.push(Part::Section(inner));
                } else {
                    parts.push(Part::Text("[".into()));
                    parts.extend(inner);
                }
            }
            ']' if in_section => {
                *pos += 1;
                flush(&mut text, &mut parts);
                return (parts, true);
            }
            c => {
                text.push(c);
                *pos += 1;
            }
        }
    }
    flush(&mut text, &mut parts);
    (parts, false)
}

/// `name%` at the start of `rest`, with a name of letters, digits and `_`.
fn field_name(rest: &[char]) -> Option<String> {
    let len = rest
        .iter()
        .position(|c| !(c.is_ascii_alphanumeric() || *c == '_'))?;
    (len > 0 && rest[len] == '%').then(|| rest[..len].iter().collect())
}

fn field(name: &str) -> Field {
    match name.to_ascii_lowercase().as_str() {
        "artist" => Field::Artist,
        "title" => Field::Title,
        "album" => Field::Album,
        "year" => Field::Year,
        "track" => Field::Track,
        "filename" => Field::Filename,
        _ => Field::Unknown,
    }
}

fn flush(text: &mut String, parts: &mut Vec<Part>) {
    if !text.is_empty() {
        parts.push(Part::Text(std::mem::take(text)));
    }
}

fn render(parts: &[Part], tags: &Tags, filename: &str, out: &mut String) {
    for part in parts {
        match part {
            Part::Text(t) => out.push_str(t),
            Part::Field(f) => out.push_str(&value(*f, tags, filename)),
            Part::Section(inner) => {
                let filled = inner.iter().all(|p| match p {
                    Part::Field(f) => !value(*f, tags, filename).is_empty(),
                    _ => true,
                });
                if filled {
                    render(inner, tags, filename, out);
                }
            }
        }
    }
}

fn value(field: Field, tags: &Tags, filename: &str) -> String {
    let text = |s: &Option<String>| s.clone().unwrap_or_default();
    let number = |n: Option<u32>| n.map(|n| n.to_string()).unwrap_or_default();
    match field {
        Field::Artist => text(&tags.artist),
        Field::Title => text(&tags.title),
        Field::Album => text(&tags.album),
        Field::Year => number(tags.year),
        Field::Track => number(tags.track),
        Field::Filename => filename.to_string(),
        Field::Unknown => String::new(),
    }
}
