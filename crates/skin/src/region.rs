//! `region.txt`: window shapes as polygons, following Webamp's `regionParser`.
//!
//! - `NumPoints` is a comma-separated list of polygon sizes; `PointList` holds x,y pairs
//!   separated by commas and/or blanks.
//! - A size below 3 skips that many points and drops the polygon.
//! - A size that is not a number drops that polygon only.
//! - A polygon cut short by the end of the list keeps the points it has; once no points
//!   are left, the remaining polygons are dropped.
//! - A polygon containing a coordinate that is not an integer is dropped (ruling: Webamp
//!   would emit an invalid shape).

use crate::ini;

pub type Polygon = Vec<(i32, i32)>;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Regions {
    pub normal: Vec<Polygon>,
    pub window_shade: Vec<Polygon>,
    pub equalizer: Vec<Polygon>,
    pub equalizer_ws: Vec<Polygon>,
}

pub fn parse(text: &str) -> Regions {
    let ini = ini::parse(text);
    let section = |name: &str| {
        ini.get(name)
            .and_then(|s| Some(polygons(s.get("numpoints")?, s.get("pointlist")?)))
            .unwrap_or_default()
    };
    Regions {
        normal: section("normal"),
        window_shade: section("windowshade"),
        equalizer: section("equalizer"),
        equalizer_ws: section("equalizerws"),
    }
}

fn polygons(num_points: &str, point_list: &str) -> Vec<Polygon> {
    let tokens: Vec<&str> = point_list
        .split([',', ' ', '\t'])
        .filter(|t| !t.is_empty())
        .collect();
    let points: Vec<Option<(i32, i32)>> = tokens
        .chunks(2)
        .map(|pair| match pair {
            [x, y] => Some((x.parse().ok()?, y.parse().ok()?)),
            _ => None,
        })
        .collect();
    let mut out = Vec::new();
    let mut next = 0usize;
    for count in num_points
        .split(',')
        .map(str::trim)
        .filter(|c| !c.is_empty())
    {
        let Ok(count) = count.parse::<usize>() else {
            continue;
        };
        if count < 3 {
            next += count;
            continue;
        }
        let Some(slice) = points.get(next..(next + count).min(points.len())) else {
            break;
        };
        if slice.is_empty() {
            break;
        }
        next += count;
        if let Some(polygon) = slice.iter().copied().collect::<Option<Polygon>>() {
            out.push(polygon);
        }
    }
    out
}
