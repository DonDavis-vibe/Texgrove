//! Binary STL: an 80-byte header, the number of triangles, and 50 bytes per triangle.

use crate::mesh::Mesh;
use crate::normals::face_normal;
use std::io::{self, Write};

/// Writes the mesh as binary STL with `name` in its header.
pub fn write<W: Write>(out: &mut W, mesh: &Mesh, name: &str) -> io::Result<()> {
    // Never starting with "solid": some programs take that for the text form of STL.
    let title = format!("Texgrove {name}");
    let mut header = [0u8; 80];
    let mut len = title.len().min(80);
    while !title.is_char_boundary(len) {
        len -= 1;
    }
    header[..len].copy_from_slice(&title.as_bytes()[..len]);
    out.write_all(&header)?;
    let count = u32::try_from(mesh.triangles.len()).map_err(|_| io::Error::other("too many triangles for an STL"))?;
    out.write_all(&count.to_le_bytes())?;
    let mut record = [0u8; 50];
    for t in 0..mesh.triangles.len() {
        let corners = mesh.corners(t);
        for (i, point) in [face_normal(mesh, t)].iter().chain(&corners).enumerate() {
            for (axis, c) in point.iter().enumerate() {
                let at = i * 12 + axis * 4;
                record[at..at + 4].copy_from_slice(&c.to_le_bytes());
            }
        }
        out.write_all(&record)?;
    }
    Ok(())
}

/// The mesh as the bytes of a binary STL.
pub fn to_bytes(mesh: &Mesh, name: &str) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(84 + mesh.triangles.len() * 50);
    write(&mut bytes, mesh, name).expect("writing into memory does not fail");
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{import, shapes};

    #[test]
    fn what_is_written_reads_back() {
        let cube = shapes::cuboid([10.0, 20.0, 30.0]);
        let bytes = to_bytes(&cube, "Würfel");
        assert_eq!(bytes.len(), 84 + 12 * 50);
        assert!(bytes.starts_with("Texgrove Würfel".as_bytes()));
        assert_eq!(&bytes[80..84], &12u32.to_le_bytes());
        // The first triangle is the bottom: its normal points down.
        assert_eq!(&bytes[84..96], &[0.0f32, 0.0, -1.0].map(f32::to_le_bytes).concat()[..]);
        let back = import::stl::read(&bytes).unwrap();
        assert_eq!(back, shapes::soup(&cube));
    }

    #[test]
    fn a_long_name_is_cut_between_letters() {
        let bytes = to_bytes(&Mesh::default(), &"ä".repeat(60));
        assert_eq!(bytes.len(), 84);
        assert!(std::str::from_utf8(&bytes[..79]).is_ok());
        assert_eq!(bytes[79], 0);
    }
}
