use serde::Serialize;

use crate::model::Hour;

/// Geometry of the hourly temperature curve, in a `width` × `height` box.
///
/// The paths use SVG path syntax, which both a web `<path d=…>` and a Slint
/// `Path { commands: … }` understand, so every UI draws the exact same curve.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ChartGeometry {
    pub width: f64,
    pub height: f64,
    /// Smooth line through all points.
    pub line: String,
    /// Same line closed down to the bottom edge, for a filled area.
    pub area: String,
    pub points: Vec<ChartPoint>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct ChartPoint {
    pub x: f64,
    pub y: f64,
    pub temperature: f64,
}

/// Lays out one point per hour, `step` apart horizontally, with the curve
/// fitted between `top_pad` and `height - bottom_pad`.
pub fn temperature_chart(
    hours: &[Hour],
    step: f64,
    height: f64,
    top_pad: f64,
    bottom_pad: f64,
) -> ChartGeometry {
    let width = step * hours.len() as f64;
    let (min, max) = hours.iter().fold((f64::MAX, f64::MIN), |(lo, hi), h| {
        (lo.min(h.temperature), hi.max(h.temperature))
    });
    let range = (max - min).max(1.0);
    let usable = height - top_pad - bottom_pad;

    let points: Vec<ChartPoint> = hours
        .iter()
        .enumerate()
        .map(|(i, h)| ChartPoint {
            x: step * (i as f64 + 0.5),
            y: top_pad + (max - h.temperature) / range * usable,
            temperature: h.temperature,
        })
        .collect();

    // The curve runs flat from the left edge to the first point and from the
    // last point to the right edge, so it spans the whole chart.
    let (line, area) = match (points.first(), points.last()) {
        (Some(first), Some(last)) => {
            let curve = smooth_path(&points);
            let line = format!(
                "M 0,{y0:.1} L {curve} L {width:.1},{y1:.1}",
                y0 = first.y,
                curve = curve.trim_start_matches("M "),
                y1 = last.y
            );
            let area = format!("{line} L {width:.1},{height:.1} L 0,{height:.1} Z");
            (line, area)
        }
        _ => (String::new(), String::new()),
    };

    ChartGeometry {
        width,
        height,
        line,
        area,
        points,
    }
}

/// Catmull-Rom spline through the points, written as cubic Bézier segments.
fn smooth_path(p: &[ChartPoint]) -> String {
    let Some(first) = p.first() else {
        return String::new();
    };
    let mut d = format!("M {:.1},{:.1}", first.x, first.y);
    for i in 0..p.len().saturating_sub(1) {
        let p0 = p[i.saturating_sub(1)];
        let (p1, p2) = (p[i], p[i + 1]);
        let p3 = p[(i + 2).min(p.len() - 1)];
        let c1 = (p1.x + (p2.x - p0.x) / 6.0, p1.y + (p2.y - p0.y) / 6.0);
        let c2 = (p2.x - (p3.x - p1.x) / 6.0, p2.y - (p3.y - p1.y) / 6.0);
        d += &format!(
            " C {:.1},{:.1} {:.1},{:.1} {:.1},{:.1}",
            c1.0, c1.1, c2.0, c2.1, p2.x, p2.y
        );
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::condition::Condition;

    fn hour(t: f64) -> Hour {
        Hour {
            time: String::new(),
            label: String::new(),
            temperature: t,
            precipitation_probability: 0.0,
            condition: Condition::Clear,
            is_day: true,
        }
    }

    #[test]
    fn warmest_hour_is_highest_on_screen() {
        let g = temperature_chart(&[hour(5.0), hour(10.0), hour(7.0)], 50.0, 100.0, 20.0, 10.0);
        assert_eq!(g.width, 150.0);
        assert_eq!(g.points[1].y, 20.0); // max → top padding
        assert_eq!(g.points[0].y, 90.0); // min → bottom padding
        assert!(g.line.starts_with("M 0,90.0 L 25.0,90.0 C"));
        assert!(g.line.ends_with("L 150.0,62.0"));
        assert!(g.area.ends_with('Z'));
    }
}
