//! The scan line algorithm behind rough.js' hatch fills (`hachure-fill`, MIT):
//! parallel lines at a fixed gap, clipped to a polygon.
//!
//! Like the original it works on rounded coordinates and rotates the polygons
//! in place and back again, which leaves rounding noise in their points that
//! the second pass of a cross-hatch sees. The code keeps both so that the
//! lines come out exactly as on excalidraw.com.

use std::cmp::Ordering;

use crate::path::Point;

/// A hatch line: where it starts and where it ends.
pub(crate) type Line = [Point; 2];

/// One edge of a polygon, as the scan line sees it.
#[derive(Debug, Clone, Copy)]
struct Edge {
    y_min: f64,
    y_max: f64,
    /// Where the edge crosses the current scan line.
    x: f64,
    /// How far `x` moves per unit of `y`.
    slope_inverse: f64,
}

/// Lines `gap` apart, tilted `angle` degrees, clipped to the area enclosed by
/// `polygons` (even-odd). Every `step`th scan line is looked at, `gap` of them
/// producing lines unless `step` is not `1`, in which case all of them do.
///
/// The polygons are rotated about the origin and back while it works.
pub(crate) fn hachure_lines(
    polygons: &mut [Vec<Point>],
    gap: f64,
    angle: f64,
    step: f64,
) -> Vec<Line> {
    let gap = gap.max(0.1);
    if angle != 0.0 {
        for polygon in polygons.iter_mut() {
            rotate(polygon, angle);
        }
    }
    let mut lines = straight_lines(polygons, gap, step);
    if angle != 0.0 {
        for polygon in polygons.iter_mut() {
            rotate(polygon, -angle);
        }
        for line in &mut lines {
            rotate(line, -angle);
        }
    }
    lines
}

fn rotate(points: &mut [Point], degrees: f64) {
    let (sin, cos) = (std::f64::consts::PI / 180.0 * degrees).sin_cos();
    for (x, y) in points {
        let (old_x, old_y) = (*x, *y);
        *x = old_x * cos - old_y * sin;
        *y = old_x * sin + old_y * cos;
    }
}

/// Horizontal hatch lines.
fn straight_lines(polygons: &[Vec<Point>], gap: f64, step: f64) -> Vec<Line> {
    let edges = edge_table(polygons);
    let mut lines = Vec::new();
    let Some(first) = edges.first() else {
        return lines;
    };
    if !step.is_finite() || step <= 0.0 {
        return lines;
    }

    let mut active: Vec<Edge> = Vec::new();
    let mut y = first.y_min;
    let mut iteration = 0.0_f64;
    let mut next_edge = 0;
    while !active.is_empty() || next_edge < edges.len() {
        // Edges that start at or above this scan line become active.
        while next_edge < edges.len() && edges[next_edge].y_min <= y {
            active.push(edges[next_edge]);
            next_edge += 1;
        }
        active.retain(|edge| edge.y_max > y);
        active.sort_by(|a, b| compare(a.x, b.x));

        if step != 1.0 || iteration % gap == 0.0 {
            // Pairs of crossings enclose the polygon; a last odd one is ignored.
            for pair in active.as_chunks::<2>().0 {
                lines.push([(js_round(pair[0].x), y), (js_round(pair[1].x), y)]);
            }
        }

        y += step;
        for edge in &mut active {
            edge.x += step * edge.slope_inverse;
        }
        iteration += 1.0;
    }
    lines
}

/// The non-horizontal edges of all polygons (closing each one), sorted by
/// where they start.
fn edge_table(polygons: &[Vec<Point>]) -> Vec<Edge> {
    let mut edges = Vec::new();
    for polygon in polygons {
        // Coordinates that are not numbers would make the scan never end.
        if polygon
            .iter()
            .any(|&(x, y)| !x.is_finite() || !y.is_finite())
        {
            continue;
        }
        // Each edge, and the one closing the polygon. hachure-fill skips
        // polygons of fewer than three points once closed, but those have
        // only level edges, which yield nothing anyway.
        let closing = polygon.last().zip(polygon.first());
        let edges_of = polygon.windows(2).map(|pair| (pair[0], pair[1]));
        for (p1, p2) in edges_of.chain(closing.map(|(&last, &first)| (last, first))) {
            if p1.1 != p2.1 {
                let y_min = p1.1.min(p2.1);
                edges.push(Edge {
                    y_min,
                    y_max: p1.1.max(p2.1),
                    x: if y_min == p1.1 { p1.0 } else { p2.0 },
                    slope_inverse: (p2.0 - p1.0) / (p2.1 - p1.1),
                });
            }
        }
    }
    edges.sort_by(|a, b| {
        compare(a.y_min, b.y_min)
            .then_with(|| compare(a.x, b.x))
            .then_with(|| compare(a.y_max, b.y_max))
    });
    edges
}

/// Orders numbers the way a JavaScript sort comparator does: anything that
/// is not clearly less or greater counts as equal.
fn compare(a: f64, b: f64) -> Ordering {
    a.partial_cmp(&b).unwrap_or(Ordering::Equal)
}

/// JavaScript's `Math.round`: halves go up, also below zero.
fn js_round(x: f64) -> f64 {
    (x + 0.5).floor()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(size: f64) -> Vec<Point> {
        vec![(0.0, 0.0), (size, 0.0), (size, size), (0.0, size)]
    }

    #[test]
    fn level_lines_run_across_the_polygon_every_gap() {
        let lines = hachure_lines(&mut [square(10.0)], 4.0, 0.0, 1.0);

        assert_eq!(
            lines,
            [
                [(0.0, 0.0), (10.0, 0.0)],
                [(0.0, 4.0), (10.0, 4.0)],
                [(0.0, 8.0), (10.0, 8.0)],
            ]
        );
    }

    #[test]
    fn a_hole_is_left_out_by_the_even_odd_rule() {
        let hole = vec![(4.0, 4.0), (6.0, 4.0), (6.0, 6.0), (4.0, 6.0)];

        let lines = hachure_lines(&mut [square(10.0), hole], 5.0, 0.0, 1.0);

        // The scan line at y = 5 crosses both squares: 0..4 and 6..10.
        assert!(lines.contains(&[(0.0, 5.0), (4.0, 5.0)]));
        assert!(lines.contains(&[(6.0, 5.0), (10.0, 5.0)]));
    }

    #[test]
    fn a_step_other_than_one_hatches_every_scan_line_taken() {
        let lines = hachure_lines(&mut [square(10.0)], 4.0, 0.0, 4.0);

        let ys: Vec<f64> = lines.iter().map(|l| l[0].1).collect();
        assert_eq!(ys, [0.0, 4.0, 8.0]);
    }

    #[test]
    fn tilted_lines_are_tilted_by_the_angle_and_stay_in_the_polygon() {
        // 90 degrees turns the horizontal lines into vertical ones.
        let lines = hachure_lines(&mut [square(10.0)], 4.0, 90.0, 1.0);

        assert!(!lines.is_empty());
        for [(x0, y0), (x1, y1)] in lines {
            assert!((x0 - x1).abs() < 1e-9, "vertical: {x0} vs {x1}");
            assert!((-1e-9..=10.0 + 1e-9).contains(&y0) && (-1e-9..=10.0 + 1e-9).contains(&y1));
        }
    }

    #[test]
    fn coordinates_are_rounded_like_javascript_rounds_them() {
        assert_eq!(js_round(2.5), 3.0);
        assert_eq!(js_round(-2.5), -2.0);
        assert_eq!(js_round(2.4), 2.0);
    }

    #[test]
    fn a_polygon_without_area_has_no_lines() {
        assert!(hachure_lines(&mut [vec![(0.0, 0.0), (10.0, 0.0)]], 4.0, 0.0, 1.0).is_empty());
        assert!(hachure_lines(&mut [], 4.0, 0.0, 1.0).is_empty());
    }
}
