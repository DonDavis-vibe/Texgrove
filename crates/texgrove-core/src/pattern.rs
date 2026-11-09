//! Textures made by formulas: each is a height from 0 to 1 at every `(u, v)` and repeats with
//! period 1 both ways, so its tiles meet without a seam.

use crate::heightmap::Heightmap;
use core::f32::consts::TAU;

/// A built-in pattern.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Pattern {
    /// Diamond knurling: small pyramids between crossing grooves.
    Knurl,
    /// V-shaped grooves side by side.
    Grooves,
    /// Rounded ribs side by side.
    Ribs,
    /// Flat stripes: half raised, half not.
    Stripes,
    /// Round bumps in a square grid.
    Dots,
    /// A honeycomb: raised hexagons with channels between.
    Hexagons,
    /// Cells like cracked clay or giraffe skin.
    Voronoi,
    /// Soft random hills.
    Noise,
    /// Bricks in running bond with mortar joints.
    Bricks,
    /// Threads over and under each other.
    Weave,
}

impl Pattern {
    pub const ALL: [Pattern; 10] = [
        Pattern::Knurl,
        Pattern::Grooves,
        Pattern::Ribs,
        Pattern::Stripes,
        Pattern::Dots,
        Pattern::Hexagons,
        Pattern::Voronoi,
        Pattern::Noise,
        Pattern::Bricks,
        Pattern::Weave,
    ];

    /// The name used in files and on the command line.
    pub fn id(self) -> &'static str {
        match self {
            Pattern::Knurl => "knurl",
            Pattern::Grooves => "grooves",
            Pattern::Ribs => "ribs",
            Pattern::Stripes => "stripes",
            Pattern::Dots => "dots",
            Pattern::Hexagons => "hexagons",
            Pattern::Voronoi => "voronoi",
            Pattern::Noise => "noise",
            Pattern::Bricks => "bricks",
            Pattern::Weave => "weave",
        }
    }

    pub fn from_id(id: &str) -> Option<Pattern> {
        Pattern::ALL.into_iter().find(|p| p.id() == id)
    }

    /// How tall one tile is compared to its width.
    pub fn aspect(self) -> f32 {
        match self {
            Pattern::Hexagons => SQRT_3,
            _ => 1.0,
        }
    }

    /// The size of the pattern's finest feature (a groove, a joint, a rounded edge) as a part of
    /// the tile's width. A mesh needs edges of about half of that to show the pattern cleanly.
    pub fn detail(self) -> f32 {
        match self {
            Pattern::Grooves => 0.5,
            Pattern::Knurl | Pattern::Ribs => 0.25,
            Pattern::Dots => 0.2,
            Pattern::Weave => 0.15,
            Pattern::Stripes => 0.08,
            Pattern::Hexagons | Pattern::Noise | Pattern::Bricks => 0.06,
            Pattern::Voronoi => 0.04,
        }
    }

    /// The height at `(u, v)`.
    pub fn height(self, u: f32, v: f32) -> f32 {
        let (u, v) = (fract(u), fract(v));
        let h = match self {
            Pattern::Knurl => tent(u + v).min(tent(u - v)),
            Pattern::Grooves => tent(u),
            Pattern::Ribs => {
                let x = 2.0 * u - 1.0;
                (1.0 - x * x).max(0.0).sqrt()
            }
            Pattern::Stripes => ramp((0.25 - (u - 0.5).abs()) / 0.04 + 0.5),
            Pattern::Dots => {
                let (x, y) = (u - 0.5, v - 0.5);
                let d2 = (x * x + y * y) / (0.36 * 0.36);
                (1.0 - d2).max(0.0).sqrt()
            }
            Pattern::Hexagons => {
                // Centres of hexagons one apart in a row, rows √3/2 apart and shifted by half.
                let y = v * SQRT_3;
                let near = |cx: f32, cy: f32| {
                    let (x, y) = ((u - cx).abs(), (y - cy).abs());
                    x.max(0.5 * x + SQRT_3 / 2.0 * y)
                };
                let d = near(0.0, 0.0)
                    .min(near(1.0, 0.0))
                    .min(near(0.5, SQRT_3 / 2.0))
                    .min(near(0.0, SQRT_3))
                    .min(near(1.0, SQRT_3));
                // 0.5 is the border between two hexagons; the channel is 0.08 wide each side.
                ramp((0.5 - 0.08 - d) / 0.06)
            }
            Pattern::Voronoi => {
                const CELLS: i32 = 5;
                let (x, y) = (u * CELLS as f32, v * CELLS as f32);
                let (cx, cy) = (libm::floorf(x) as i32, libm::floorf(y) as i32);
                let (mut first, mut second) = (f32::INFINITY, f32::INFINITY);
                for j in cy - 1..=cy + 1 {
                    for i in cx - 1..=cx + 1 {
                        let (wi, wj) = (i.rem_euclid(CELLS) as u32, j.rem_euclid(CELLS) as u32);
                        let px = i as f32 + 0.15 + 0.7 * unit(hash(wi, wj, 1));
                        let py = j as f32 + 0.15 + 0.7 * unit(hash(wi, wj, 2));
                        let d = libm::sqrtf((x - px) * (x - px) + (y - py) * (y - py));
                        if d < first {
                            (first, second) = (d, first);
                        } else if d < second {
                            second = d;
                        }
                    }
                }
                // Near a border the two nearest seeds are equally far: a groove there.
                ramp((second - first) / 0.25)
            }
            Pattern::Noise => {
                let mut sum = 0.0;
                let mut weight = 0.0;
                for octave in 0..4u32 {
                    let cells = 4u32 << octave;
                    let amplitude = 1.0 / (1u32 << octave) as f32;
                    sum += amplitude * value_noise(u, v, cells, octave);
                    weight += amplitude;
                }
                // Sums of several octaves crowd the middle; spread them out again.
                ramp((sum / weight - 0.5) * 1.8 + 0.5)
            }
            Pattern::Bricks => {
                // Two courses per tile, the upper one shifted by half a brick.
                let course = if v < 0.5 { 0.0 } else { 0.5 };
                let along = fract(u + course);
                let across = fract(v * 2.0);
                let joint = 0.06;
                let edge = along.min(1.0 - along).min((across.min(1.0 - across)) * 0.5);
                ramp((edge - joint * 0.5) / 0.03)
            }
            Pattern::Weave => {
                // Two threads each way per tile; where they cross, one goes over the other.
                let mut h = 0.0f32;
                for k in 0..2 {
                    let phase = k as f32 * 0.5;
                    let centre = 0.25 + 0.5 * k as f32;
                    let profile = |across: f32| {
                        let x = (across - centre) / 0.23;
                        (1.0 - x * x).max(0.0).sqrt()
                    };
                    let lift = |along: f32| 0.5 + 0.5 * libm::cosf(TAU * (along - 0.25 + phase));
                    h = h.max(profile(v) * (0.35 + 0.65 * lift(u)));
                    h = h.max(profile(u) * (0.35 + 0.65 * (1.0 - lift(v))));
                }
                h
            }
        };
        h.clamp(0.0, 1.0)
    }

    /// The pattern as a height map of `size` cells along `u` (and as many along `v` as its
    /// aspect asks for).
    pub fn render(self, size: u32) -> Heightmap {
        let size = size.max(2);
        let rows = ((size as f32 * self.aspect()).round() as u32).max(2);
        Heightmap::from_fn(size, rows, |u, v| self.height(u, v)).with_aspect(self.aspect())
    }
}

const SQRT_3: f32 = 1.732_050_8;

#[inline]
fn fract(x: f32) -> f32 {
    x - libm::floorf(x)
}

/// A tent over every whole number: 0 at `n`, 1 at `n + ½`.
#[inline]
fn tent(x: f32) -> f32 {
    1.0 - (2.0 * fract(x) - 1.0).abs()
}

/// 0 below 0, 1 above 1, and a smooth rise between.
#[inline]
fn ramp(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// A scrambled number for a grid cell: the same on every machine.
#[inline]
fn hash(x: u32, y: u32, salt: u32) -> u32 {
    let mut h = x.wrapping_mul(0x9E37_79B1) ^ y.wrapping_mul(0x85EB_CA77) ^ salt.wrapping_mul(0xC2B2_AE3D);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297A_2D39);
    h ^ (h >> 15)
}

/// A hash as a number from 0 to 1.
#[inline]
fn unit(h: u32) -> f32 {
    (h >> 8) as f32 / (1u32 << 24) as f32
}

/// Random values at the corners of a grid of `cells` squares per tile, blended smoothly between.
fn value_noise(u: f32, v: f32, cells: u32, salt: u32) -> f32 {
    let (x, y) = (u * cells as f32, v * cells as f32);
    let (x0, y0) = (libm::floorf(x), libm::floorf(y));
    let (fx, fy) = (ramp(x - x0), ramp(y - y0));
    let corner = |dx: u32, dy: u32| {
        unit(hash((x0 as u32).wrapping_add(dx) % cells, (y0 as u32).wrapping_add(dy) % cells, salt + 10))
    };
    let below = corner(0, 0) * (1.0 - fx) + corner(1, 0) * fx;
    let above = corner(0, 1) * (1.0 - fx) + corner(1, 1) * fx;
    below * (1.0 - fy) + above * fy
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_pattern_stays_in_range_uses_it_and_repeats() {
        for pattern in Pattern::ALL {
            let (mut low, mut high) = (f32::INFINITY, f32::NEG_INFINITY);
            for j in 0..64 {
                for i in 0..64 {
                    let (u, v) = ((i as f32 + 0.37) / 64.0, (j as f32 + 0.61) / 64.0);
                    let h = pattern.height(u, v);
                    assert!((0.0..=1.0).contains(&h), "{pattern:?} at {u} {v}: {h}");
                    low = low.min(h);
                    high = high.max(h);
                    for (du, dv) in [(1.0, 0.0), (0.0, 1.0), (-2.0, 3.0)] {
                        let again = pattern.height(u + du, v + dv);
                        assert!((again - h).abs() < 1e-3, "{pattern:?} does not repeat at {u} {v}: {h} {again}");
                    }
                }
            }
            assert!(low < 0.2 && high > 0.85, "{pattern:?} spans only {low}..{high}");
            assert_eq!(Pattern::from_id(pattern.id()), Some(pattern));
        }
        assert_eq!(Pattern::from_id("nope"), None);
    }

    #[test]
    fn tiles_meet_without_a_step() {
        // Just left and right of a tile's edge, and just below and above it.
        for pattern in Pattern::ALL {
            for i in 0..97 {
                let t = i as f32 / 97.0;
                let across_u = (pattern.height(-1e-4, t) - pattern.height(1e-4, t)).abs();
                let across_v = (pattern.height(t, -1e-4) - pattern.height(t, 1e-4)).abs();
                assert!(across_u < 0.02 && across_v < 0.02, "{pattern:?} at {t}: {across_u} {across_v}");
            }
        }
    }

    #[test]
    fn a_knurl_is_pyramids_and_a_honeycomb_is_taller_than_wide() {
        assert_eq!(Pattern::Knurl.height(0.5, 0.0), 1.0);
        assert_eq!(Pattern::Knurl.height(0.0, 0.0), 0.0);
        assert_eq!(Pattern::Knurl.height(0.25, 0.0), 0.5);
        let comb = Pattern::Hexagons.render(64);
        assert_eq!((comb.width(), comb.height()), (64, 111));
        assert!((comb.aspect - 1.732).abs() < 1e-3);
        // The middle of a hexagon is up, the border between two is down.
        assert_eq!(Pattern::Hexagons.height(0.0, 0.0), 1.0);
        assert_eq!(Pattern::Hexagons.height(0.5, 0.0), 0.0);
        assert_eq!(Pattern::Hexagons.height(0.5, 0.5), 1.0);
    }

    #[test]
    fn rendering_is_the_same_every_time() {
        assert_eq!(Pattern::Voronoi.render(48), Pattern::Voronoi.render(48));
        assert_eq!(Pattern::Noise.render(48).values().len(), 48 * 48);
    }
}
