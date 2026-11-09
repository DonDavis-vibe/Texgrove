//! Choosing how fine to work, so nobody has to think in edge lengths and triangle counts.

use crate::pipeline::Finish;

/// How fine the result should be.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Quality {
    /// Quick, for trying a texture out.
    Draft,
    /// For printing.
    #[default]
    Normal,
    /// For small textures, resin printers and close looks.
    Fine,
}

/// The finish for a quality: how fine to refine and how far to reduce.
///
/// `feature` is the size in mm of the finest thing the textures on the model show (a tile's
/// width times the texture's [`detail`](crate::heightmap::Heightmap::detail), the smallest over
/// all layers) and `nozzle` the printer's nozzle width in mm. A printer cannot show what is much
/// finer than its nozzle, so there is no point in a mesh that is.
pub fn finish(quality: Quality, feature: f32, nozzle: f32) -> Finish {
    let nozzle = if nozzle.is_finite() && nozzle > 0.0 { nozzle.clamp(0.05, 2.0) } else { 0.4 };
    let feature = if feature.is_finite() && feature > 0.0 { feature } else { 1.0 };
    // Edges per feature, the shortest edge worth having as a part of the nozzle, the budget, and
    // what may be taken away unseen as a part of the nozzle.
    let (per_feature, finest, triangles, unseen) = match quality {
        Quality::Draft => (1.5, 0.75, 300_000, 0.05),
        Quality::Normal => (2.0, 0.5, 1_000_000, 0.025),
        Quality::Fine => (3.0, 0.3, 3_000_000, 0.0125),
    };
    Finish {
        max_edge: (feature / per_feature).max(finest * nozzle),
        triangles,
        tolerance: unseen * nozzle,
        // While refining, a few times the budget is enough to choose from.
        max_triangles: (triangles * 8).min(10_000_000),
        ..Finish::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finer_means_shorter_edges_more_triangles_and_less_taken_away() {
        let (draft, normal, fine) =
            (finish(Quality::Draft, 1.5, 0.4), finish(Quality::Normal, 1.5, 0.4), finish(Quality::Fine, 1.5, 0.4));
        assert_eq!((draft.max_edge, normal.max_edge, fine.max_edge), (1.0, 0.75, 0.5));
        assert!(draft.triangles < normal.triangles && normal.triangles < fine.triangles);
        assert!(draft.tolerance > normal.tolerance && normal.tolerance > fine.tolerance);
        assert!((normal.tolerance - 0.01).abs() < 1e-6);
        assert!(fine.max_triangles <= 10_000_000 && draft.max_triangles >= draft.triangles);
    }

    #[test]
    fn the_nozzle_sets_a_floor() {
        // A texture with features of 0.1 mm: the mesh does not go below half a 0.4 mm nozzle…
        assert_eq!(finish(Quality::Normal, 0.1, 0.4).max_edge, 0.2);
        // …but does for a resin printer's 0.05 mm.
        assert_eq!(finish(Quality::Normal, 0.1, 0.05).max_edge, 0.05);
        // Nonsense in, something sensible out.
        let odd = finish(Quality::Normal, f32::NAN, -1.0);
        assert!(odd.max_edge == 0.5 && (odd.tolerance - 0.01).abs() < 1e-6);
    }
}
