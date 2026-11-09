//! The little vector arithmetic the kernel needs, on plain arrays.

/// A point or direction in millimetres.
pub type P3 = [f32; 3];

/// The same in double precision, for sums over many triangles.
pub type D3 = [f64; 3];

#[inline]
pub fn add(a: P3, b: P3) -> P3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

#[inline]
pub fn sub(a: P3, b: P3) -> P3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

#[inline]
pub fn scale(a: P3, s: f32) -> P3 {
    [a[0] * s, a[1] * s, a[2] * s]
}

#[inline]
pub fn dot(a: P3, b: P3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

#[inline]
pub fn cross(a: P3, b: P3) -> P3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

#[inline]
pub fn length(a: P3) -> f32 {
    dot(a, a).sqrt()
}

#[inline]
pub fn distance(a: P3, b: P3) -> f32 {
    length(sub(a, b))
}

/// The direction of `a` with length one; zero stays zero.
#[inline]
pub fn normalize(a: P3) -> P3 {
    let len = length(a);
    if len > 0.0 { scale(a, 1.0 / len) } else { [0.0; 3] }
}

#[inline]
pub fn lerp(a: P3, b: P3, t: f32) -> P3 {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

#[inline]
pub fn min(a: P3, b: P3) -> P3 {
    [a[0].min(b[0]), a[1].min(b[1]), a[2].min(b[2])]
}

#[inline]
pub fn max(a: P3, b: P3) -> P3 {
    [a[0].max(b[0]), a[1].max(b[1]), a[2].max(b[2])]
}

#[inline]
pub fn to64(a: P3) -> D3 {
    [f64::from(a[0]), f64::from(a[1]), f64::from(a[2])]
}

#[inline]
pub fn sub64(a: D3, b: D3) -> D3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

#[inline]
pub fn dot64(a: D3, b: D3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

#[inline]
pub fn cross64(a: D3, b: D3) -> D3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

#[inline]
pub fn length64(a: D3) -> f64 {
    dot64(a, a).sqrt()
}

/// The point of the triangle `a b c` closest to `p`.
pub fn closest_on_triangle(p: P3, a: P3, b: P3, c: P3) -> P3 {
    // Ericson, Real-Time Collision Detection, 5.1.5: walk the regions of the triangle's plane.
    let (ab, ac, ap) = (sub(b, a), sub(c, a), sub(p, a));
    let (d1, d2) = (dot(ab, ap), dot(ac, ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = sub(p, b);
    let (d3, d4) = (dot(ab, bp), dot(ac, bp));
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return add(a, scale(ab, d1 / (d1 - d3)));
    }
    let cp = sub(p, c);
    let (d5, d6) = (dot(ab, cp), dot(ac, cp));
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return add(a, scale(ac, d2 / (d2 - d6)));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        return add(b, scale(sub(c, b), w));
    }
    let denom = va + vb + vc;
    if denom == 0.0 {
        return a; // a triangle without area
    }
    let (v, w) = (vb / denom, vc / denom);
    add(a, add(scale(ab, v), scale(ac, w)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_closest_point_lies_in_the_face_on_an_edge_or_at_a_corner() {
        let (a, b, c) = ([0.0, 0.0, 0.0], [4.0, 0.0, 0.0], [0.0, 4.0, 0.0]);
        assert_eq!(closest_on_triangle([1.0, 1.0, 5.0], a, b, c), [1.0, 1.0, 0.0]);
        assert_eq!(closest_on_triangle([2.0, -3.0, 1.0], a, b, c), [2.0, 0.0, 0.0]);
        assert_eq!(closest_on_triangle([-1.0, -1.0, 0.0], a, b, c), a);
        assert_eq!(closest_on_triangle([9.0, -1.0, 0.0], a, b, c), b);
        assert_eq!(closest_on_triangle([3.0, 3.0, 2.0], a, b, c), [2.0, 2.0, 0.0]);
    }

    #[test]
    fn cross_follows_the_right_hand() {
        assert_eq!(cross([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]), [0.0, 0.0, 1.0]);
        assert_eq!(normalize([0.0, 3.0, 4.0]), [0.0, 0.6, 0.8]);
        assert_eq!(normalize([0.0; 3]), [0.0; 3]);
    }
}
