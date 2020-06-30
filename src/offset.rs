use super::*;
use geo_booleanop::boolean::BooleanOp;

use std::error;
use std::fmt;

/// If offset computing fails this error is returned.
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum OffsetError {
    /// This error can be produced when manipulating edges.
    EdgeError(EdgeError),
    UnknownGeometry,
}

impl error::Error for OffsetError {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self {
            Self::EdgeError(e) => Some(e),
            _ => None,
        }
    }
}

impl fmt::Display for OffsetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EdgeError(e) => write!(f, "Edge error: {}", e),
            Self::UnknownGeometry => write!(f, "Unknown geometry"),
        }
    }
}

/// Arcs around corners are made of 5 segments by default.
pub const DEFAULT_ARC_SEGMENTS: u32 = 5;

pub trait Offset {
    fn offset(&self, distance: f64) -> Result<geo::MultiPolygon<f64>, OffsetError> {
        self.offset_with_arc_segments(distance, DEFAULT_ARC_SEGMENTS)
    }

    fn offset_with_arc_segments(
        &self,
        distance: f64,
        arc_segments: u32,
    ) -> Result<geo::MultiPolygon<f64>, OffsetError>;
}

impl Offset for geo::GeometryCollection<f64> {
    fn offset_with_arc_segments(
        &self,
        distance: f64,
        arc_segments: u32,
    ) -> Result<geo::MultiPolygon<f64>, OffsetError> {
        let mut geometry_collection_with_offset = geo::MultiPolygon(Vec::new());
        for geometry in self.0.iter() {
            let geometry_with_offset = geometry.offset_with_arc_segments(distance, arc_segments)?;
            geometry_collection_with_offset =
                geometry_collection_with_offset.union(&geometry_with_offset);
        }
        Ok(geometry_collection_with_offset)
    }
}

impl Offset for geo::Geometry<f64> {
    fn offset_with_arc_segments(
        &self,
        distance: f64,
        arc_segments: u32,
    ) -> Result<geo::MultiPolygon<f64>, OffsetError> {
        match self {
            geo::Geometry::Point(point) => point.offset_with_arc_segments(distance, arc_segments),
            geo::Geometry::Line(line) => line.offset_with_arc_segments(distance, arc_segments),
            geo::Geometry::LineString(line_tring) => {
                line_tring.offset_with_arc_segments(distance, arc_segments)
            }
            geo::Geometry::Polygon(polygon) => {
                polygon.offset_with_arc_segments(distance, arc_segments)
            }
            geo::Geometry::MultiPoint(multi_point) => {
                multi_point.offset_with_arc_segments(distance, arc_segments)
            }
            geo::Geometry::MultiLineString(multi_line_string) => {
                multi_line_string.offset_with_arc_segments(distance, arc_segments)
            }
            geo::Geometry::MultiPolygon(multi_polygon) => {
                multi_polygon.offset_with_arc_segments(distance, arc_segments)
            }
            geo::Geometry::GeometryCollection(geometry_collection) => {
                geometry_collection.offset_with_arc_segments(distance, arc_segments)
            }
            _ => Err(OffsetError::UnknownGeometry),
        }
    }
}

impl Offset for geo::MultiPolygon<f64> {
    fn offset_with_arc_segments(
        &self,
        distance: f64,
        arc_segments: u32,
    ) -> Result<geo::MultiPolygon<f64>, OffsetError> {
        let mut polygons = geo::MultiPolygon(Vec::new());
        for polygon in self.0.iter() {
            let polygon_with_offset = polygon.offset_with_arc_segments(distance, arc_segments)?;
            polygons = polygons.union(&polygon_with_offset);
        }
        Ok(polygons)
    }
}

impl Offset for geo::Polygon<f64> {
    fn offset_with_arc_segments(
        &self,
        distance: f64,
        arc_segments: u32,
    ) -> Result<geo::MultiPolygon<f64>, OffsetError> {
        let exterior_with_offset = self
            .exterior()
            .offset_with_arc_segments(distance.abs(), arc_segments)?;
        let interiors_with_offset = geo::MultiLineString(self.interiors().to_vec())
            .offset_with_arc_segments(distance.abs(), arc_segments)?;

        Ok(if distance.is_sign_positive() {
            self.union(&exterior_with_offset)
                .union(&interiors_with_offset)
        } else {
            self.difference(&exterior_with_offset)
                .difference(&interiors_with_offset)
        })
    }
}

impl Offset for geo::MultiLineString<f64> {
    fn offset_with_arc_segments(
        &self,
        distance: f64,
        arc_segments: u32,
    ) -> Result<geo::MultiPolygon<f64>, OffsetError> {
        if distance < 0.0 {
            return Ok(geo::MultiPolygon(Vec::new()));
        }

        let mut multi_line_string_with_offset = geo::MultiPolygon(Vec::new());
        for line_string in self.0.iter() {
            let line_string_with_offset =
                line_string.offset_with_arc_segments(distance, arc_segments)?;
            multi_line_string_with_offset =
                multi_line_string_with_offset.union(&line_string_with_offset);
        }
        Ok(multi_line_string_with_offset)
    }
}

impl Offset for geo::LineString<f64> {
    fn offset_with_arc_segments(
        &self,
        distance: f64,
        arc_segments: u32,
    ) -> Result<geo::MultiPolygon<f64>, OffsetError> {
        if distance < 0.0 {
            return Ok(geo::MultiPolygon(Vec::new()));
        }

        let mut line_string_with_offset = geo::MultiPolygon(Vec::new());
        for line in self.lines() {
            let line_with_offset = line.offset_with_arc_segments(distance, arc_segments)?;
            line_string_with_offset = line_string_with_offset.union(&line_with_offset);
        }

        let line_string_with_offset = line_string_with_offset.0.iter().skip(1).fold(
            geo::MultiPolygon(
                line_string_with_offset
                    .0
                    .get(0)
                    .map(|polygon| vec![polygon.clone()])
                    .unwrap_or_default(),
            ),
            |result, hole| result.difference(hole),
        );

        Ok(line_string_with_offset)
    }
}

impl Offset for geo::Line<f64> {
    fn offset_with_arc_segments(
        &self,
        distance: f64,
        arc_segments: u32,
    ) -> Result<geo::MultiPolygon<f64>, OffsetError> {
        if distance < 0.0 {
            return Ok(geo::MultiPolygon(Vec::new()));
        }

        let v1 = &self.start;
        let v2 = &self.end;
        let e1 = Edge::new(v1, v2);

        if let (Ok(in_normal), Ok(out_normal)) = (e1.inwards_normal(), e1.outwards_normal()) {
            let offsets = [
                e1.with_offset(in_normal.x * distance, in_normal.y * distance),
                e1.inverse_with_offset(out_normal.x * distance, out_normal.y * distance),
            ];

            let len = 2;
            let mut vertices = Vec::new();

            for i in 0..len {
                let current_edge = offsets.get(i).unwrap();
                let prev_edge = offsets.get((i + len + 1) % len).unwrap();
                create_arc(
                    &mut vertices,
                    if i == 0 { v1 } else { v2 },
                    distance,
                    &prev_edge.next,
                    &current_edge.current,
                    arc_segments,
                    true,
                );
            }

            Ok(geo::MultiPolygon(vec![geo::Polygon::new(
                geo::LineString(vertices),
                vec![],
            )]))
        } else {
            geo::Point::from(self.start).offset_with_arc_segments(distance, arc_segments)
        }
    }
}

impl Offset for geo::MultiPoint<f64> {
    fn offset_with_arc_segments(
        &self,
        distance: f64,
        arc_segments: u32,
    ) -> Result<geo::MultiPolygon<f64>, OffsetError> {
        if distance < 0.0 {
            return Ok(geo::MultiPolygon(Vec::new()));
        }

        let mut multi_point_with_offset = geo::MultiPolygon(Vec::new());
        for point in self.0.iter() {
            let point_with_offset = point.offset_with_arc_segments(distance, arc_segments)?;
            multi_point_with_offset = multi_point_with_offset.union(&point_with_offset);
        }
        Ok(multi_point_with_offset)
    }
}

impl Offset for geo::Point<f64> {
    fn offset_with_arc_segments(
        &self,
        distance: f64,
        arc_segments: u32,
    ) -> Result<geo::MultiPolygon<f64>, OffsetError> {
        if distance < 0.0 {
            return Ok(geo::MultiPolygon(Vec::new()));
        }

        let mut angle = 0.0;

        let vertice_count = match arc_segments * 2 {
            count if count % 2 == 0 => count + 1,
            count => count,
        };

        let contour = (0..vertice_count)
            .map(|_| {
                angle += 2.0 * std::f64::consts::PI / f64::from(vertice_count); // counter-clockwise
                geo::Coordinate::from((
                    self.x() + (distance * angle.cos()),
                    self.y() + (distance * angle.sin()),
                ))
            })
            .collect();

        Ok(geo::MultiPolygon(vec![geo::Polygon::new(
            contour,
            Vec::new(),
        )]))
    }
}

fn create_arc(
    vertices: &mut Vec<geo::Coordinate<f64>>,
    center: &geo::Coordinate<f64>,
    radius: f64,
    start_vertex: &geo::Coordinate<f64>,
    end_vertex: &geo::Coordinate<f64>,
    segment_count: u32,
    outwards: bool,
) {
    let pi2 = std::f64::consts::PI * 2.0;

    let start_angle = (start_vertex.y - center.y).atan2(start_vertex.x - center.x);
    let start_angle = if start_angle.is_sign_negative() {
        start_angle + pi2
    } else {
        start_angle
    };

    let end_angle = (end_vertex.y - center.y).atan2(end_vertex.x - center.x);
    let end_angle = if end_angle.is_sign_negative() {
        end_angle + pi2
    } else {
        end_angle
    };

    let segment_count = if segment_count % 2 == 0 {
        segment_count - 1
    } else {
        segment_count
    };

    let angle = if start_angle > end_angle {
        start_angle - end_angle
    } else {
        start_angle + pi2 - end_angle
    };

    let segment_angle = if outwards { -angle } else { pi2 - angle } / f64::from(segment_count);

    vertices.push(*start_vertex);
    for i in 1..segment_count {
        let angle = start_angle + segment_angle * f64::from(i);
        vertices.push(geo::Coordinate::from((
            center.x + angle.cos() * radius,
            center.y + angle.sin() * radius,
        )));
    }
    vertices.push(*end_vertex);
}
