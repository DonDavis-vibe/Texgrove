//! Reading STL, binary or text.

use crate::mesh::Mesh;
use crate::vec::P3;

/// Why bytes are not an STL.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum StlError {
    #[error("the file is neither a binary nor a text STL")]
    NotStl,
    #[error("the file ends in the middle of triangle {0}")]
    CutShort(usize),
    #[error("a number in the file cannot be read: {0}")]
    BadNumber(String),
    #[error("a coordinate is not a finite number")]
    NotFinite,
}

/// Reads an STL as a triangle soup: every triangle with its own vertices, to be joined by
/// [`crate::weld::weld`]. The normals in the file are ignored; the order of the corners counts.
pub fn read(bytes: &[u8]) -> Result<Mesh, StlError> {
    let triangles = match binary_count(bytes) {
        Some(count) => read_binary(bytes, count),
        None if looks_like_text(bytes) => read_text(bytes)?,
        None => return Err(StlError::NotStl),
    };
    if triangles.as_flattened().as_flattened().iter().any(|c| !c.is_finite()) {
        return Err(StlError::NotFinite);
    }
    Ok(Mesh::from_soup(&triangles))
}

/// The number of triangles if the length fits a binary STL. Binary files may begin with "solid"
/// too, so the length decides; a few stray bytes at the end are let pass.
fn binary_count(bytes: &[u8]) -> Option<usize> {
    let count = u32::from_le_bytes(bytes.get(80..84)?.try_into().ok()?) as usize;
    let needed = 84 + count.checked_mul(50)?;
    (bytes.len() >= needed && bytes.len() - needed < 50 && !(count == 0 && looks_like_text(bytes))).then_some(count)
}

fn looks_like_text(bytes: &[u8]) -> bool {
    bytes.trim_ascii_start().starts_with(b"solid")
}

fn read_binary(bytes: &[u8], count: usize) -> Vec<[P3; 3]> {
    let (records, _) = bytes[84..84 + count * 50].as_chunks::<50>();
    records
        .iter()
        .map(|record| {
            [1, 2, 3].map(|corner| {
                [0, 1, 2].map(|axis| {
                    let at = corner * 12 + axis * 4;
                    f32::from_le_bytes([record[at], record[at + 1], record[at + 2], record[at + 3]])
                })
            })
        })
        .collect()
}

fn read_text(bytes: &[u8]) -> Result<Vec<[P3; 3]>, StlError> {
    let text = String::from_utf8_lossy(bytes);
    let mut words = text.split_ascii_whitespace();
    let mut points: Vec<P3> = Vec::new();
    while let Some(word) = words.next() {
        if !word.eq_ignore_ascii_case("vertex") {
            continue;
        }
        let mut point = [0.0f32; 3];
        for c in &mut point {
            let word = words.next().ok_or(StlError::CutShort(points.len() / 3))?;
            *c = word.parse().map_err(|_| StlError::BadNumber(word.chars().take(40).collect()))?;
        }
        points.push(point);
    }
    let (triangles, rest) = points.as_chunks::<3>();
    if !rest.is_empty() {
        return Err(StlError::CutShort(triangles.len()));
    }
    Ok(triangles.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT: &str = "solid test\n facet normal 0 0 1\n  outer loop\n   vertex 0 0 0\n   vertex 1e1 0 0\n   \
                        vertex 0 2.5 0\n  endloop\n endfacet\nendsolid test\n";

    #[test]
    fn text_stl_reads() {
        let mesh = read(TEXT.as_bytes()).unwrap();
        assert_eq!(mesh.positions, vec![[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [0.0, 2.5, 0.0]]);
        assert_eq!(mesh.triangles, vec![[0, 1, 2]]);
    }

    #[test]
    fn a_binary_file_may_start_with_solid() {
        let mut bytes = vec![0u8; 84 + 50];
        bytes[..5].copy_from_slice(b"solid");
        bytes[80] = 1;
        bytes[84 + 12 + 12..84 + 12 + 16].copy_from_slice(&3.0f32.to_le_bytes());
        let mesh = read(&bytes).unwrap();
        assert_eq!(mesh.positions[1], [3.0, 0.0, 0.0]);
    }

    #[test]
    fn broken_files_say_why() {
        assert_eq!(read(b"PK\x03\x04 not an stl"), Err(StlError::NotStl));
        assert_eq!(read(b"solid x\n vertex 1 2\n"), Err(StlError::CutShort(0)));
        assert_eq!(read(b"solid x\n vertex 1 2 3\n vertex 1 2 3\n"), Err(StlError::CutShort(0)));
        assert_eq!(read(b"solid x\n vertex 1 zwei 3\n"), Err(StlError::BadNumber("zwei".into())));
        assert_eq!(read(b"solid x\n vertex 1 nan 3 vertex 0 0 0 vertex 1 1 1\n"), Err(StlError::NotFinite));
        // A binary header promising more triangles than the file holds.
        let mut bytes = vec![0u8; 84 + 50];
        bytes[80] = 2;
        assert_eq!(read(&bytes), Err(StlError::NotStl));
        // An empty text STL is an empty mesh.
        assert_eq!(read(b"solid empty\nendsolid empty\n").unwrap(), Mesh::default());
    }
}
