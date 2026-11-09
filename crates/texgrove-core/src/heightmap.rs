//! A texture as heights: a grid of values from 0 (deepest) to 1 (highest) that repeats in both
//! directions.

/// How a height map continues beyond its edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tiling {
    /// Tile after tile, each the same.
    #[default]
    Repeat,
    /// Every other tile mirrored, so even a picture that does not repeat meets itself without a
    /// step.
    Mirror,
}

/// Why values are not a height map.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum HeightmapError {
    #[error("a height map needs at least one value each way")]
    Empty,
    #[error("{width} × {height} values were announced, {found} came")]
    WrongLength { width: u32, height: u32, found: usize },
    #[error("a height is not a finite number")]
    NotFinite,
}

/// Heights on a grid. `u` runs along a row, `v` from row to row; one tile spans 0 to 1 each way.
#[derive(Debug, Clone, PartialEq)]
pub struct Heightmap {
    width: u32,
    height: u32,
    values: Vec<f32>,
    /// How tall one tile is compared to its width when laid on a surface: 1 for a square tile,
    /// √3 for a honeycomb's.
    pub aspect: f32,
}

impl Heightmap {
    /// From `width × height` values, row by row. Values outside 0–1 are cut to it.
    pub fn new(width: u32, height: u32, mut values: Vec<f32>) -> Result<Heightmap, HeightmapError> {
        if width == 0 || height == 0 {
            return Err(HeightmapError::Empty);
        }
        if values.len() != width as usize * height as usize {
            return Err(HeightmapError::WrongLength { width, height, found: values.len() });
        }
        if values.iter().any(|v| !v.is_finite()) {
            return Err(HeightmapError::NotFinite);
        }
        for v in &mut values {
            *v = v.clamp(0.0, 1.0);
        }
        Ok(Heightmap { width, height, values, aspect: 1.0 })
    }

    /// From a function of `(u, v)`, asked at the middle of every cell.
    pub fn from_fn(width: u32, height: u32, f: impl Fn(f32, f32) -> f32) -> Heightmap {
        let (width, height) = (width.max(1), height.max(1));
        let mut values = Vec::with_capacity(width as usize * height as usize);
        for y in 0..height {
            let v = (y as f32 + 0.5) / height as f32;
            for x in 0..width {
                let value = f((x as f32 + 0.5) / width as f32, v);
                values.push(if value.is_finite() { value.clamp(0.0, 1.0) } else { 0.0 });
            }
        }
        Heightmap { width, height, values, aspect: 1.0 }
    }

    /// From a greyscale picture, row by row from the top: white is high. The picture stands
    /// upright on a wall, so its top row gets the largest `v`.
    pub fn from_grey(width: u32, height: u32, grey: &[u8]) -> Result<Heightmap, HeightmapError> {
        if width == 0 || height == 0 {
            return Err(HeightmapError::Empty);
        }
        if grey.len() != width as usize * height as usize {
            return Err(HeightmapError::WrongLength { width, height, found: grey.len() });
        }
        let values = grey.rchunks(width as usize).flatten().map(|&g| f32::from(g) / 255.0).collect();
        Ok(Heightmap { width, height, values, aspect: height as f32 / width as f32 })
    }

    pub fn with_aspect(mut self, aspect: f32) -> Heightmap {
        self.aspect = if aspect.is_finite() && aspect > 0.0 { aspect } else { 1.0 };
        self
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// The values, row by row from `v = 0`.
    pub fn values(&self) -> &[f32] {
        &self.values
    }

    /// The values as a greyscale picture, row by row from the top.
    pub fn to_grey(&self) -> Vec<u8> {
        self.values.rchunks(self.width as usize).flatten().map(|v| (v * 255.0).round() as u8).collect()
    }

    /// The value of a cell, counted on beyond the edges as the tiling says.
    #[inline]
    fn cell(&self, x: i64, y: i64, tiling: Tiling) -> f32 {
        let fold = |i: i64, n: u32| -> usize {
            let n = i64::from(n);
            match tiling {
                Tiling::Repeat => i.rem_euclid(n) as usize,
                Tiling::Mirror => {
                    let i = i.rem_euclid(2 * n);
                    (if i < n { i } else { 2 * n - 1 - i }) as usize
                }
            }
        };
        self.values[fold(y, self.height) * self.width as usize + fold(x, self.width)]
    }

    /// The height at `(u, v)`, blended from the four cells around.
    #[inline]
    pub fn sample(&self, u: f32, v: f32, tiling: Tiling) -> f32 {
        let x = u * self.width as f32 - 0.5;
        let y = v * self.height as f32 - 0.5;
        let (x0, y0) = (x.floor(), y.floor());
        let (fx, fy) = (x - x0, y - y0);
        let (x0, y0) = (x0 as i64, y0 as i64);
        let below = self.cell(x0, y0, tiling) * (1.0 - fx) + self.cell(x0 + 1, y0, tiling) * fx;
        let above = self.cell(x0, y0 + 1, tiling) * (1.0 - fx) + self.cell(x0 + 1, y0 + 1, tiling) * fx;
        below * (1.0 - fy) + above * fy
    }

    /// The map smoothed over about `radius` cells (three passes of a running mean, which is close
    /// to a Gaussian blur), running round the edges as the tiling says.
    pub fn blurred(&self, radius: f32, tiling: Tiling) -> Heightmap {
        let radius = radius.round().clamp(0.0, 4096.0) as i64;
        if radius == 0 {
            return self.clone();
        }
        let mut map = self.clone();
        let mut line = Vec::new();
        for _ in 0..3 {
            let source = map.clone();
            for y in 0..self.height as i64 {
                line.clear();
                line.extend((-radius..self.width as i64 + radius).map(|x| source.cell(x, y, tiling)));
                running_mean(&line, radius as usize, |x, value| {
                    map.values[y as usize * self.width as usize + x] = value;
                });
            }
            let source = map.clone();
            for x in 0..self.width as i64 {
                line.clear();
                line.extend((-radius..self.height as i64 + radius).map(|y| source.cell(x, y, tiling)));
                running_mean(&line, radius as usize, |y, value| {
                    map.values[y * self.width as usize + x as usize] = value;
                });
            }
        }
        map
    }
}

/// The mean of every `2 × radius + 1` neighbours in `line`, which is longer than the result by
/// `radius` at each end.
fn running_mean(line: &[f32], radius: usize, mut put: impl FnMut(usize, f32)) {
    let window = 2 * radius + 1;
    let mut sum: f64 = line[..window].iter().map(|&v| f64::from(v)).sum();
    put(0, (sum / window as f64) as f32);
    for i in 1..=line.len() - window {
        sum += f64::from(line[i + window - 1]) - f64::from(line[i - 1]);
        put(i, (sum / window as f64) as f32);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_are_checked_and_cut() {
        assert_eq!(Heightmap::new(0, 2, vec![]), Err(HeightmapError::Empty));
        assert_eq!(
            Heightmap::new(2, 2, vec![0.0; 3]),
            Err(HeightmapError::WrongLength { width: 2, height: 2, found: 3 })
        );
        assert_eq!(Heightmap::new(1, 1, vec![f32::NAN]), Err(HeightmapError::NotFinite));
        assert_eq!(Heightmap::new(2, 1, vec![-1.0, 7.0]).unwrap().values(), &[0.0, 1.0]);
    }

    #[test]
    fn sampling_blends_and_repeats() {
        // Two columns: 0 on the left, 1 on the right.
        let map = Heightmap::new(2, 1, vec![0.0, 1.0]).unwrap();
        assert_eq!(map.sample(0.25, 0.5, Tiling::Repeat), 0.0);
        assert_eq!(map.sample(0.75, 0.5, Tiling::Repeat), 1.0);
        assert_eq!(map.sample(0.5, 0.5, Tiling::Repeat), 0.5);
        // At the edge of the tile the next tile's first column comes in.
        assert_eq!(map.sample(1.0, 0.5, Tiling::Repeat), 0.5);
        assert_eq!(map.sample(0.0, 0.5, Tiling::Repeat), 0.5);
        // Far away and on the negative side the tile is the same.
        assert!((map.sample(-7.25, 3.5, Tiling::Repeat) - 1.0).abs() < 1e-5);
        assert!((map.sample(100.25, 0.5, Tiling::Repeat) - 0.0).abs() < 1e-4);
    }

    #[test]
    fn mirrored_tiles_meet_without_a_step() {
        let map = Heightmap::new(2, 1, vec![0.0, 1.0]).unwrap();
        // The next tile starts with this one's last column.
        assert_eq!(map.sample(1.0, 0.5, Tiling::Mirror), 1.0);
        assert_eq!(map.sample(1.25, 0.5, Tiling::Mirror), 1.0);
        assert_eq!(map.sample(1.75, 0.5, Tiling::Mirror), 0.0);
        assert_eq!(map.sample(2.25, 0.5, Tiling::Mirror), 0.0);
        assert_eq!(map.sample(-0.25, 0.5, Tiling::Mirror), 0.0);
    }

    #[test]
    fn a_grey_picture_stands_upright() {
        // Top row white, bottom row black.
        let map = Heightmap::from_grey(1, 2, &[255, 0]).unwrap();
        assert_eq!(map.values(), &[0.0, 1.0]);
        assert_eq!(map.aspect, 2.0);
        assert_eq!(map.to_grey(), vec![255, 0]);
        assert_eq!(map.sample(0.5, 0.75, Tiling::Repeat), 1.0);
    }

    #[test]
    fn blurring_keeps_the_mean_and_flattens_steps() {
        let map = Heightmap::from_fn(32, 32, |u, _| if u < 0.5 { 0.0 } else { 1.0 });
        let soft = map.blurred(2.0, Tiling::Repeat);
        let mean = |m: &Heightmap| m.values().iter().sum::<f32>() / m.values().len() as f32;
        assert!((mean(&soft) - mean(&map)).abs() < 1e-5);
        let step = |m: &Heightmap| {
            m.sample(0.5 + 0.5 / 32.0, 0.5, Tiling::Repeat) - m.sample(0.5 - 0.5 / 32.0, 0.5, Tiling::Repeat)
        };
        assert_eq!(step(&map), 1.0);
        assert!(step(&soft) < 0.3 && step(&soft) > 0.0, "{}", step(&soft));
        // Every row is the same before, so it is after: the blur along v changed nothing.
        assert!((soft.sample(0.3, 0.1, Tiling::Repeat) - soft.sample(0.3, 0.9, Tiling::Repeat)).abs() < 1e-6);
        assert_eq!(map.blurred(0.2, Tiling::Repeat), map);
    }
}
