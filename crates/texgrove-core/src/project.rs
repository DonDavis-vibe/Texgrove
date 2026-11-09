//! Laying a flat texture onto a model: for a point of the surface, where on the height map is it?
//!
//! The preview and the export both ask [`Projector::height`], so they cannot disagree.

use crate::heightmap::{Heightmap, Tiling};
use crate::mesh::{Bounds, Mesh};
use crate::vec::{self, P3};
use core::f32::consts::{PI, TAU};
use std::sync::Arc;

/// The way a texture is laid on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Mapping {
    /// From all three axes at once, each counting by how much the surface faces it. Works on
    /// any shape without setting anything up.
    Triplanar {
        /// How quickly one direction takes over from another: 1 blends widely, 8 and more
        /// nearly switches.
        sharpness: f32,
    },
    /// Like a box round the model: every point takes the texture from the side it faces most.
    Cubic {
        /// How wide the band is where two sides blend at an edge of the box: 0 switches hard,
        /// 1 blends over the whole way between two sides.
        blend: f32,
    },
    /// Wrapped round the vertical (z) axis like a label round a bottle; the top and bottom get
    /// it from above and below.
    Cylindrical {
        /// Surfaces facing up or down within this many degrees count as top and bottom.
        cap_degrees: f32,
        /// Over this many degrees round the cap angle, side and cap blend.
        cap_blend: f32,
    },
    /// Wrapped round the vertical axis along the part's outline seen from above, instead of by
    /// angle: on a box-like part a pattern keeps its size along the flat sides and runs on
    /// round the corners. A whole number of tiles goes round. Where the outline cannot be
    /// followed from the centre (a C shape), the outer envelope is followed. Needs the mesh:
    /// set up with [`Projector::for_mesh`], else it is laid on like `Cylindrical`.
    Outline { cap_degrees: f32, cap_blend: f32 },
    /// The texture once, like a label or a logo: centred at `centre`, facing along `normal`
    /// (from the surface outwards), a tile wide. Only what faces the stamp gets it.
    Stamp { centre: P3, normal: P3 },
    /// Wrapped round the model's centre like a map on a globe.
    Spherical,
    /// Straight from above (and below).
    PlanarXY,
    /// Straight from the front (and back).
    PlanarXZ,
    /// Straight from the side.
    PlanarYZ,
}

impl Default for Mapping {
    fn default() -> Self {
        Mapping::Triplanar { sharpness: 4.0 }
    }
}

/// Where and how large the texture lies.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    /// The width of one tile in mm.
    pub tile: f32,
    /// Stretches the tile's height: 1 keeps the texture's own proportions.
    pub stretch: f32,
    /// Shifts the texture, in mm along its width and height.
    pub offset: [f32; 2],
    /// Turns the texture, in degrees counter-clockwise.
    pub rotation: f32,
    pub tiling: Tiling,
}

impl Default for Placement {
    fn default() -> Self {
        Placement { tile: 5.0, stretch: 1.0, offset: [0.0; 2], rotation: 0.0, tiling: Tiling::Repeat }
    }
}

/// The outline of a part seen from above, as arc length by angle round its centre.
#[derive(Debug, Clone)]
struct Outline {
    /// Arc length from angle 0 at each of [`OUTLINE_STEPS`] equal steps of angle, and once more
    /// at the end: the whole way round.
    arc: Vec<f32>,
}

/// Steps of angle the outline is measured in.
const OUTLINE_STEPS: usize = 1440;

impl Outline {
    /// The outer envelope of the mesh seen from above round `centre`: for every step of angle,
    /// the farthest the mesh reaches, sampled along its edges; lightly smoothed, then measured.
    fn of(mesh: &Mesh, centre: P3) -> Option<Outline> {
        let mut reach = vec![0.0f32; OUTLINE_STEPS];
        let step = TAU / OUTLINE_STEPS as f32;
        let bin = |x: f32, y: f32| {
            let angle = libm::atan2f(y - centre[1], x - centre[0]) + PI;
            ((angle / step) as usize).min(OUTLINE_STEPS - 1)
        };
        let mut note = |x: f32, y: f32| {
            let r = libm::sqrtf((x - centre[0]).powi(2) + (y - centre[1]).powi(2));
            let k = bin(x, y);
            reach[k] = reach[k].max(r);
        };
        for t in &mesh.triangles {
            for k in 0..3 {
                let (a, b) = (mesh.positions[t[k] as usize], mesh.positions[t[(k + 1) % 3] as usize]);
                // Enough points along the edge that no step of angle is skipped.
                let (ra, rb) = (
                    libm::sqrtf((a[0] - centre[0]).powi(2) + (a[1] - centre[1]).powi(2)),
                    libm::sqrtf((b[0] - centre[0]).powi(2) + (b[1] - centre[1]).powi(2)),
                );
                let length = libm::sqrtf((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2));
                let near = ra.min(rb).max(1e-3);
                let points = ((length / (near * step)).ceil() as usize).clamp(1, OUTLINE_STEPS);
                for i in 0..=points {
                    let t = i as f32 / points as f32;
                    note(a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t);
                }
            }
        }
        if reach.iter().all(|r| *r <= 0.0) {
            return None;
        }
        // Steps the mesh does not reach (a gap in a C shape seen from inside) take their
        // neighbours' reach, so the envelope stays closed.
        for _ in 0..OUTLINE_STEPS {
            let mut filled = true;
            for k in 0..OUTLINE_STEPS {
                if reach[k] <= 0.0 {
                    let (l, r) = (reach[(k + OUTLINE_STEPS - 1) % OUTLINE_STEPS], reach[(k + 1) % OUTLINE_STEPS]);
                    reach[k] = l.max(r);
                    filled = false;
                }
            }
            if filled {
                break;
            }
        }
        // A light running mean over three degrees: a corner stays a corner, a jag goes.
        let window = OUTLINE_STEPS / 240;
        let smooth: Vec<f32> = (0..OUTLINE_STEPS)
            .map(|k| {
                let sum: f32 = (0..=2 * window).map(|j| reach[(k + OUTLINE_STEPS + j - window) % OUTLINE_STEPS]).sum();
                sum / (2 * window + 1) as f32
            })
            .collect();
        let point = |k: usize| {
            let angle = (k % OUTLINE_STEPS) as f32 * step - PI;
            let r = smooth[k % OUTLINE_STEPS];
            [r * libm::cosf(angle), r * libm::sinf(angle)]
        };
        let mut arc = Vec::with_capacity(OUTLINE_STEPS + 1);
        let mut length = 0.0f32;
        arc.push(0.0);
        for k in 0..OUTLINE_STEPS {
            let (a, b) = (point(k), point(k + 1));
            length += libm::sqrtf((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2));
            arc.push(length);
        }
        (length > 0.0).then_some(Outline { arc })
    }

    fn perimeter(&self) -> f32 {
        *self.arc.last().unwrap_or(&0.0)
    }

    /// The arc length at an angle from 0 to 2π.
    fn along(&self, angle: f32) -> f32 {
        let x = (angle / TAU * OUTLINE_STEPS as f32).clamp(0.0, OUTLINE_STEPS as f32);
        let k = (x as usize).min(OUTLINE_STEPS - 1);
        let t = x - k as f32;
        self.arc[k] + (self.arc[k + 1] - self.arc[k]) * t
    }
}

/// A mapping set up for one model.
#[derive(Debug, Clone)]
pub struct Projector {
    mapping: Mapping,
    tiling: Tiling,
    /// The corner of the model's box the texture starts from.
    origin: P3,
    centre: P3,
    /// The radius the texture is wrapped at, for the round mappings.
    radius: f32,
    /// One over the tile's width and height in mm.
    per_mm: [f32; 2],
    /// The same for the way round: the width adjusted so a whole number of tiles goes round.
    per_mm_round: f32,
    offset: [f32; 2],
    sin: f32,
    cos: f32,
    /// Where cap and side meet for the round mappings: the angle from straight up or down, and
    /// the width of the band they blend over, in degrees.
    cap_band: (f32, f32),
    /// For [`Mapping::Outline`], when it could be measured.
    outline: Option<Arc<Outline>>,
    /// For [`Mapping::Stamp`]: two unit vectors across the stamp (its width and height).
    across: (P3, P3),
}

impl Projector {
    /// Sets a mapping up for a model with these bounds and a texture whose tile is `aspect`
    /// times as tall as wide. Use the bounds of the model as loaded, not of a refined or moved
    /// copy, or the texture shifts.
    pub fn new(mapping: Mapping, placement: &Placement, bounds: &Bounds, aspect: f32) -> Projector {
        Projector::build(mapping, placement, bounds, aspect, None)
    }

    /// [`Projector::new`] for this mesh (the model as loaded): the outline mapping measures it.
    pub fn for_mesh(mapping: Mapping, placement: &Placement, mesh: &Mesh, aspect: f32) -> Projector {
        let bounds = mesh.bounds();
        let outline = match mapping {
            Mapping::Outline { .. } if !bounds.is_empty() => Outline::of(mesh, bounds.centre()).map(Arc::new),
            _ => None,
        };
        Projector::build(mapping, placement, &bounds, aspect, outline)
    }

    fn build(
        mapping: Mapping,
        placement: &Placement,
        bounds: &Bounds,
        aspect: f32,
        outline: Option<Arc<Outline>>,
    ) -> Projector {
        let tile = if placement.tile.is_finite() { placement.tile.max(1e-3) } else { 5.0 };
        let stretch = if placement.stretch.is_finite() { placement.stretch.max(1e-3) } else { 1.0 };
        let tile_height = tile * stretch * if aspect.is_finite() && aspect > 0.0 { aspect } else { 1.0 };
        let size = bounds.size();
        let radius = match mapping {
            Mapping::Spherical => 0.5 * size[0].max(size[1]).max(size[2]),
            _ => 0.5 * size[0].max(size[1]),
        }
        .max(1e-3);
        // Round the model a whole number of tiles, so the texture meets itself.
        let around = outline.as_ref().map_or(TAU * radius, |o| o.perimeter());
        let tiles_round = libm::roundf(around / tile).max(1.0);
        let (sin, cos) = (libm::sinf(placement.rotation.to_radians()), libm::cosf(placement.rotation.to_radians()));
        let (cap_degrees, cap_blend) = match mapping {
            Mapping::Cylindrical { cap_degrees, cap_blend } | Mapping::Outline { cap_degrees, cap_blend } => {
                (cap_degrees.clamp(0.0, 90.0), cap_blend.clamp(0.0, 90.0))
            }
            _ => (0.0, 0.0),
        };
        let cap_band = (cap_degrees, cap_blend);
        let across = match mapping {
            Mapping::Stamp { normal, .. } => stamp_frame(normal),
            _ => ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
        };
        Projector {
            mapping,
            tiling: placement.tiling,
            origin: if bounds.is_empty() { [0.0; 3] } else { bounds.min },
            centre: if bounds.is_empty() { [0.0; 3] } else { bounds.centre() },
            radius,
            per_mm: [1.0 / tile, 1.0 / tile_height],
            per_mm_round: tiles_round / around,
            offset: placement.offset,
            sin,
            cos,
            cap_band,
            outline,
            across,
        }
    }

    /// How many tiles go round the model with the round mappings.
    pub fn tiles_round(&self) -> u32 {
        let around = self.outline.as_ref().map_or(TAU * self.radius, |o| o.perimeter());
        libm::roundf(self.per_mm_round * around) as u32
    }

    /// How much of the texture a point gets for the mapping alone: 1 everywhere, except for a
    /// stamp, which covers its square (fading out over its rim) on surfaces facing it.
    pub fn cover(&self, p: P3, normal: P3) -> f32 {
        let Mapping::Stamp { centre, normal: facing } = self.mapping else { return 1.0 };
        let toward = vec::dot(normal, vec::normalize(facing));
        if toward <= 0.15 {
            return 0.0;
        }
        let d = vec::sub(p, centre);
        let (a, b) = self.turned_raw(vec::dot(d, self.across.0), vec::dot(d, self.across.1));
        let (u, v) = (a * self.per_mm[0], b * self.per_mm[1]);
        // A rim of 4 % of the stamp fades it out; a surface turning away fades it too.
        let edge = |x: f32| ((0.5 - x.abs()) / 0.04).clamp(0.0, 1.0);
        edge(u) * edge(v) * ((toward - 0.15) / 0.2).clamp(0.0, 1.0)
    }

    /// The height of the texture (0 to 1) at a point of the surface with the given unit normal.
    pub fn height(&self, map: &Heightmap, p: P3, normal: P3) -> f32 {
        let [x, y, z] = [p[0] - self.origin[0], p[1] - self.origin[1], p[2] - self.origin[2]];
        let facing = |n: f32| if n < 0.0 { -1.0 } else { 1.0 };
        // Each side seen from outside, the texture upright and not mirrored.
        let from_x = |s: &Self| s.flat(map, facing(normal[0]) * y, z);
        let from_y = |s: &Self| s.flat(map, -facing(normal[1]) * x, z);
        let from_z = |s: &Self| s.flat(map, x, facing(normal[2]) * y);
        match self.mapping {
            Mapping::PlanarXY => from_z(self),
            Mapping::PlanarXZ => from_y(self),
            Mapping::PlanarYZ => from_x(self),
            Mapping::Cubic { blend } => {
                let a = normal.map(f32::abs);
                let most = a[0].max(a[1]).max(a[2]);
                // Sides within the band of the one faced most take a share, fading with distance.
                let band = blend.clamp(0.0, 1.0) * 0.5;
                let weights = a.map(|x| {
                    if band <= 0.0 { f32::from(x >= most) } else { ((x - (most - band)) / band).clamp(0.0, 1.0) }
                });
                let total = weights[0] + weights[1] + weights[2];
                if total <= 0.0 {
                    return from_z(self);
                }
                if band <= 0.0 {
                    // Hard switch: the first side faced most, as before.
                    return if a[2] >= a[0] && a[2] >= a[1] {
                        from_z(self)
                    } else if a[0] >= a[1] {
                        from_x(self)
                    } else {
                        from_y(self)
                    };
                }
                let mut sum = 0.0;
                if weights[0] > 0.0 {
                    sum += weights[0] * from_x(self);
                }
                if weights[1] > 0.0 {
                    sum += weights[1] * from_y(self);
                }
                if weights[2] > 0.0 {
                    sum += weights[2] * from_z(self);
                }
                (sum / total).clamp(0.0, 1.0)
            }
            Mapping::Triplanar { sharpness } => {
                let power = sharpness.clamp(1.0, 64.0);
                let weights = normal.map(|n| libm::powf(n.abs(), power));
                let total = weights[0] + weights[1] + weights[2];
                if total <= 0.0 {
                    return from_z(self); // no normal: any side is as good
                }
                let mut sum = 0.0;
                // A side that hardly counts is not worth looking up.
                if weights[0] > 1e-3 * total {
                    sum += weights[0] * from_x(self);
                }
                if weights[1] > 1e-3 * total {
                    sum += weights[1] * from_y(self);
                }
                if weights[2] > 1e-3 * total {
                    sum += weights[2] * from_z(self);
                }
                (sum / total).clamp(0.0, 1.0)
            }
            Mapping::Cylindrical { .. } | Mapping::Outline { .. } => {
                // How much of the cap's texture: 1 on the cap, 0 on the side, blended between, by
                // the angle from straight up or down.
                let from_up = libm::acosf(normal[2].abs().min(1.0)).to_degrees();
                let (at, width) = self.cap_band;
                let cap = if width <= 0.0 {
                    f32::from(from_up < at)
                } else {
                    let t = ((at + width / 2.0 - from_up) / width).clamp(0.0, 1.0);
                    t * t * (3.0 - 2.0 * t)
                };
                let side = || {
                    let along = match &self.outline {
                        Some(outline) => outline.along(self.angle(p)),
                        None => self.angle(p) * self.radius,
                    };
                    self.round(map, along, z)
                };
                match cap {
                    c if c >= 1.0 => from_z(self),
                    c if c <= 0.0 => side(),
                    c => c * from_z(self) + (1.0 - c) * side(),
                }
            }
            Mapping::Stamp { centre, .. } => {
                let d = vec::sub(p, centre);
                let (a, b) = self.turned_raw(vec::dot(d, self.across.0), vec::dot(d, self.across.1));
                // Centred on the stamp, once: outside it the edge repeats, and `cover` is 0 there.
                let (u, v) = ((a * self.per_mm[0] + 0.5).clamp(0.0, 1.0), (b * self.per_mm[1] + 0.5).clamp(0.0, 1.0));
                map.sample(u, v, Tiling::Mirror)
            }
            Mapping::Spherical => {
                let d = [p[0] - self.centre[0], p[1] - self.centre[1], p[2] - self.centre[2]];
                let reach = libm::sqrtf(d[0] * d[0] + d[1] * d[1] + d[2] * d[2]);
                let latitude = if reach > 0.0 { libm::asinf((d[2] / reach).clamp(-1.0, 1.0)) } else { 0.0 };
                self.round(map, self.angle(p) * self.radius, (latitude + PI / 2.0) * self.radius)
            }
        }
    }

    /// The angle of a point round the vertical axis through the model's centre, from 0 to 2π.
    #[inline]
    fn angle(&self, p: P3) -> f32 {
        libm::atan2f(p[1] - self.centre[1], p[0] - self.centre[0]) + PI
    }

    /// The height at a place given in mm on a flat side.
    #[inline]
    fn flat(&self, map: &Heightmap, a: f32, b: f32) -> f32 {
        let (a, b) = self.turned(a, b);
        map.sample(a * self.per_mm[0], b * self.per_mm[1], self.tiling)
    }

    /// The height at a place given in mm round the model and along it.
    #[inline]
    fn round(&self, map: &Heightmap, along: f32, up: f32) -> f32 {
        let (a, b) = self.turned(along, up);
        // Only a quarter turn keeps the whole number of tiles round the model; any other
        // rotation shows a seam where the angle starts over.
        map.sample(a * self.per_mm_round, b * self.per_mm[1], self.tiling)
    }

    #[inline]
    fn turned(&self, a: f32, b: f32) -> (f32, f32) {
        let (a, b) = self.turned_raw(a, b);
        (a - self.offset[0], b - self.offset[1])
    }

    #[inline]
    fn turned_raw(&self, a: f32, b: f32) -> (f32, f32) {
        (a * self.cos + b * self.sin, -a * self.sin + b * self.cos)
    }
}

/// Two unit vectors across a stamp facing along `normal`: the first level, the second as near
/// to "up" as the surface allows, so a stamp on a wall stands upright.
fn stamp_frame(normal: P3) -> (P3, P3) {
    let n = vec::normalize(normal);
    if n == [0.0; 3] {
        return ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
    }
    // On a top or bottom face "up" is the y axis, as seen from above.
    let up = if n[2].abs() > 0.9 { [0.0, 1.0, 0.0] } else { [0.0, 0.0, 1.0] };
    let first = vec::normalize(vec::cross(up, n));
    (first, vec::cross(n, first))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pattern::Pattern;

    fn bounds(size: P3) -> Bounds {
        Bounds { min: [0.0; 3], max: size }
    }

    /// Stripes along the first direction: the height tells where across a tile a place lies.
    fn stripes() -> Heightmap {
        Pattern::Grooves.render(256)
    }

    #[test]
    fn a_flat_side_measures_in_millimetres() {
        let map = stripes();
        let placement = Placement { tile: 4.0, ..Placement::default() };
        let top = Projector::new(Mapping::PlanarXY, &placement, &bounds([40.0; 3]), 1.0);
        let up = [0.0, 0.0, 1.0];
        // A groove every 4 mm along x, whatever y and z are: 0 at the tile's edge, 1 in its middle.
        assert!(top.height(&map, [0.0, 3.0, 9.0], up) < 0.02);
        assert!(top.height(&map, [2.0, 7.0, 1.0], up) > 0.98);
        assert!((top.height(&map, [1.0, 0.0, 0.0], up) - 0.5).abs() < 0.02);
        assert!((top.height(&map, [13.0, 0.0, 0.0], up) - 0.5).abs() < 0.02);
        // Shifted by 1 mm, the middle of the tile is at 3 mm.
        let shifted = Placement { offset: [1.0, 0.0], ..placement };
        let top = Projector::new(Mapping::PlanarXY, &shifted, &bounds([40.0; 3]), 1.0);
        assert!(top.height(&map, [3.0, 0.0, 0.0], up) > 0.98);
        // Turned a quarter, the grooves run along x and are counted along y.
        let turned = Placement { rotation: 90.0, ..placement };
        let top = Projector::new(Mapping::PlanarXY, &turned, &bounds([40.0; 3]), 1.0);
        assert!(top.height(&map, [17.3, 2.0, 0.0], up) > 0.98);
        assert!(top.height(&map, [17.3, 4.0, 0.0], up) < 0.02);
    }

    #[test]
    fn the_texture_starts_at_the_models_corner_wherever_the_model_is() {
        let map = stripes();
        let placement = Placement { tile: 4.0, ..Placement::default() };
        let here = Projector::new(Mapping::PlanarXY, &placement, &bounds([40.0; 3]), 1.0);
        let moved = Bounds { min: [101.3, -7.7, 3.0], max: [141.3, 32.3, 43.0] };
        let there = Projector::new(Mapping::PlanarXY, &placement, &moved, 1.0);
        let up = [0.0, 0.0, 1.0];
        for i in 0..40 {
            let x = i as f32 * 0.37;
            let (a, b) = (here.height(&map, [x, 1.0, 0.0], up), there.height(&map, [x + 101.3, -6.7, 3.0], up));
            assert!((a - b).abs() < 1e-3, "{a} {b}");
        }
    }

    #[test]
    fn a_box_takes_each_side_from_its_own_direction() {
        let map = stripes();
        let placement = Placement { tile: 4.0, ..Placement::default() };
        let cubic = Projector::new(Mapping::Cubic { blend: 0.0 }, &placement, &bounds([40.0; 3]), 1.0);
        // On the right side the grooves are counted along y, on the front along x.
        assert!(cubic.height(&map, [40.0, 2.0, 5.0], [1.0, 0.0, 0.0]) > 0.98);
        assert!(cubic.height(&map, [40.0, 4.0, 5.0], [1.0, 0.0, 0.0]) < 0.02);
        assert!(cubic.height(&map, [2.0, 0.0, 5.0], [0.0, -1.0, 0.0]) > 0.98);
        // Seen from outside the texture is not mirrored: on the left side it runs the other way.
        let ramp = Heightmap::from_fn(64, 1, |u, _| u);
        let left = cubic.height(&ramp, [0.0, 1.0, 0.0], [-1.0, 0.0, 0.0]);
        let right = cubic.height(&ramp, [40.0, 1.0, 0.0], [1.0, 0.0, 0.0]);
        assert!((right - 0.25).abs() < 0.02 && (left - 0.75).abs() < 0.02, "{left} {right}");

        // Triplanar agrees on a face and blends on a slope.
        let tri = Projector::new(Mapping::default(), &placement, &bounds([40.0; 3]), 1.0);
        assert!(tri.height(&map, [40.0, 2.0, 5.0], [1.0, 0.0, 0.0]) > 0.98);
        let d = core::f32::consts::FRAC_1_SQRT_2;
        let between = tri.height(&map, [2.0, 4.0, 5.0], [d, d, 0.0]);
        assert!((between - 0.5).abs() < 0.02, "{between}"); // 0 from the side, 1 from the front
    }

    #[test]
    fn round_a_cylinder_goes_a_whole_number_of_tiles() {
        let map = stripes();
        let placement = Placement { tile: 4.0, ..Placement::default() };
        // 20 mm across: 62.83 mm round, so 16 tiles of 3.93 mm.
        let round = Projector::new(
            Mapping::Cylindrical { cap_degrees: 30.0, cap_blend: 0.0 },
            &placement,
            &bounds([20.0, 20.0, 50.0]),
            1.0,
        );
        assert_eq!(round.tiles_round(), 16);
        let at = |degrees: f32, z: f32| {
            let (s, c) = (libm::sinf(degrees.to_radians()), libm::cosf(degrees.to_radians()));
            round.height(&map, [10.0 + 10.0 * c, 10.0 + 10.0 * s, z], [c, s, 0.0])
        };
        // One tile is 22.5 degrees; the texture repeats with it and does not care about z.
        for degrees in [3.0f32, 77.0, 141.0, -100.0] {
            assert!((at(degrees, 5.0) - at(degrees + 22.5, 31.0)).abs() < 0.02);
        }
        // Where the angle starts over there is no step.
        assert!((at(179.99, 0.0) - at(-179.99, 0.0)).abs() < 0.02);
        // The top is a cap: flat from above.
        let top = Projector::new(Mapping::PlanarXY, &placement, &bounds([20.0, 20.0, 50.0]), 1.0);
        let up = [0.0, 0.0, 1.0];
        assert_eq!(round.height(&map, [6.0, 9.0, 50.0], up), top.height(&map, [6.0, 9.0, 50.0], up));
    }

    #[test]
    fn a_globe_has_no_step_where_the_angle_starts_over() {
        let map = Pattern::Dots.render(128);
        let placement = Placement { tile: 6.0, ..Placement::default() };
        let globe = Projector::new(Mapping::Spherical, &placement, &bounds([30.0; 3]), 1.0);
        for i in 1..20 {
            let latitude = (i as f32 / 20.0 - 0.5) * PI;
            let at = |degrees: f32| {
                let (s, c) = (libm::sinf(degrees.to_radians()), libm::cosf(degrees.to_radians()));
                let n = [c * libm::cosf(latitude), s * libm::cosf(latitude), libm::sinf(latitude)];
                globe.height(&map, [15.0 + 15.0 * n[0], 15.0 + 15.0 * n[1], 15.0 + 15.0 * n[2]], n)
            };
            assert!((at(179.99) - at(-179.99)).abs() < 0.05);
        }
    }

    #[test]
    fn a_box_blends_its_sides_near_an_edge() {
        let ramp = Heightmap::from_fn(256, 1, |u, _| u);
        let placement = Placement { tile: 40.0, ..Placement::default() };
        let hard = Projector::new(Mapping::Cubic { blend: 0.0 }, &placement, &bounds([40.0; 3]), 1.0);
        let soft = Projector::new(Mapping::Cubic { blend: 0.5 }, &placement, &bounds([40.0; 3]), 1.0);
        // Facing almost exactly between the front and the right side.
        let n = vec::normalize([1.0, -0.98, 0.0]);
        let (side, front) = (
            hard.height(&ramp, [40.0, 10.0, 0.0], [1.0, 0.0, 0.0]),
            hard.height(&ramp, [40.0, 10.0, 0.0], [0.0, -1.0, 0.0]),
        );
        let blended = soft.height(&ramp, [40.0, 10.0, 0.0], n);
        assert!(blended > side.min(front) + 0.01 && blended < side.max(front) - 0.01, "{side} {front} {blended}");
        // Facing one side squarely, blending changes nothing.
        assert_eq!(soft.height(&ramp, [40.0, 10.0, 3.0], [1.0, 0.0, 0.0]), side);
    }

    #[test]
    fn the_cap_of_a_cylinder_blends_into_its_side() {
        let map = stripes();
        let placement = Placement { tile: 4.0, ..Placement::default() };
        let mapping = Mapping::Cylindrical { cap_degrees: 30.0, cap_blend: 20.0 };
        let round = Projector::new(mapping, &placement, &bounds([20.0, 20.0, 50.0]), 1.0);
        let at = |degrees_from_up: f32| {
            let t = degrees_from_up.to_radians();
            round.height(&map, [3.0, 10.0, 25.0], [libm::sinf(t), 0.0, libm::cosf(t)])
        };
        let top = Projector::new(Mapping::PlanarXY, &placement, &bounds([20.0, 20.0, 50.0]), 1.0);
        let cap = top.height(&map, [3.0, 10.0, 25.0], [0.0, 0.0, 1.0]);
        let side = at(80.0);
        assert_eq!(at(10.0), cap); // well inside the cap
        assert!((at(30.0) - (cap + side) / 2.0).abs() < 0.02, "{} {cap} {side}", at(30.0));
    }

    #[test]
    fn round_a_box_the_outline_keeps_the_tile_size_on_flat_sides() {
        // A box 60 × 20 mm: by angle its long sides would stretch the tiles, along the outline not.
        let mesh = crate::shapes::cuboid([60.0, 20.0, 10.0]);
        let map = stripes();
        let placement = Placement { tile: 4.0, ..Placement::default() };
        let outline =
            Projector::for_mesh(Mapping::Outline { cap_degrees: 30.0, cap_blend: 0.0 }, &placement, &mesh, 1.0);
        // 160 mm round (a little less: corners are smoothed): 40 tiles.
        assert!((39..=40).contains(&outline.tiles_round()), "{}", outline.tiles_round());
        let front = [0.0, -1.0, 0.0];
        // Along the front, one groove every 4 mm (the tile is shrunk a little to fit).
        let grooves: Vec<f32> = (0..=60)
            .map(|i| i as f32 * 0.25 + 20.0)
            .filter(|x| outline.height(&map, [*x, 0.0, 5.0], front) < 0.05)
            .collect();
        let steps: Vec<f32> = grooves.windows(2).map(|w| w[1] - w[0]).filter(|d| *d > 1.0).collect();
        assert!(!steps.is_empty() && steps.iter().all(|d| (d - 4.0).abs() < 0.6), "{steps:?}");
        // Without the mesh it falls back to the cylinder.
        let plain =
            Projector::new(Mapping::Outline { cap_degrees: 30.0, cap_blend: 0.0 }, &placement, &mesh.bounds(), 1.0);
        assert_eq!(plain.tiles_round(), 47);
    }

    #[test]
    fn a_stamp_covers_its_square_on_what_faces_it() {
        let ramp = Heightmap::from_fn(256, 1, |u, _| u);
        let placement = Placement { tile: 10.0, ..Placement::default() };
        let stamp = Mapping::Stamp { centre: [20.0, 0.0, 15.0], normal: [0.0, -1.0, 0.0] };
        let projector = Projector::new(stamp, &placement, &bounds([40.0, 40.0, 30.0]), 1.0);
        let front = [0.0, -1.0, 0.0];
        assert_eq!(projector.cover([20.0, 0.0, 15.0], front), 1.0);
        assert_eq!(projector.cover([26.0, 0.0, 15.0], front), 0.0); // beyond its half width
        assert_eq!(projector.cover([20.0, 40.0, 15.0], [0.0, 1.0, 0.0]), 0.0); // the back faces away
        // Across the stamp the ramp runs from 0 to 1, left to right as seen from the front.
        let left = projector.height(&ramp, [16.0, 0.0, 15.0], front);
        let right = projector.height(&ramp, [24.0, 0.0, 15.0], front);
        assert!((left - 0.1).abs() < 0.02 && (right - 0.9).abs() < 0.02, "{left} {right}");
        // On a wall the stamp stands upright: its second direction is up.
        let (_, up) = stamp_frame([0.0, -1.0, 0.0]);
        assert_eq!(up, [0.0, 0.0, 1.0]);
    }

    #[test]
    fn odd_settings_do_not_break_it() {
        let map = stripes();
        let odd = Placement { tile: 0.0, stretch: f32::NAN, ..Placement::default() };
        let projector = Projector::new(Mapping::default(), &odd, &Bounds::EMPTY, f32::NAN);
        let h = projector.height(&map, [1.0, 2.0, 3.0], [0.0; 3]);
        assert!((0.0..=1.0).contains(&h));
    }
}
