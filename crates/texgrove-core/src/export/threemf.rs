//! 3MF: a zip package with the model as XML, each part an object of its own with its name.
//!
//! The same input always gives the same bytes: entries carry a fixed date, and numbers are
//! written in their shortest form that reads back exactly.

use crate::mesh::Mesh;
use std::fmt::Write as _;
use std::io::{self, Cursor, Seek, Write};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipWriter};

/// One part of the model.
#[derive(Debug, Clone, Copy)]
pub struct Object<'a> {
    pub name: &'a str,
    pub mesh: &'a Mesh,
    /// Where the part sits: the rows of a 3×3 matrix and then the shift, as 3MF writes
    /// transforms. `None` leaves it where its coordinates put it.
    pub transform: Option<[f32; 12]>,
}

const CONTENT_TYPES: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
    <Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">\
    <Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>\
    <Default Extension=\"model\" ContentType=\"application/vnd.ms-package.3dmanufacturing-3dmodel+xml\"/>\
    </Types>\n";

const RELATIONSHIPS: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
    <Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
    <Relationship Target=\"/3D/3dmodel.model\" Id=\"rel0\" \
    Type=\"http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel\"/>\
    </Relationships>\n";

/// Text is handed to the compressor in pieces of about this size.
const PIECE: usize = 1 << 16;

/// Writes a 3MF with the given parts into `out` and hands `out` back.
pub fn write<W: Write + Seek>(out: W, title: &str, objects: &[Object]) -> io::Result<W> {
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .last_modified_time(DateTime::default())
        .unix_permissions(0o644);
    let mut zip = ZipWriter::new(out);
    zip.start_file("[Content_Types].xml", options).map_err(io::Error::other)?;
    zip.write_all(CONTENT_TYPES.as_bytes())?;
    zip.start_file("_rels/.rels", options).map_err(io::Error::other)?;
    zip.write_all(RELATIONSHIPS.as_bytes())?;
    // A model of millions of triangles is far more than 4 GiB of text only in theory, but the
    // flag costs 20 bytes and saves a failed export.
    zip.start_file("3D/3dmodel.model", options.large_file(true)).map_err(io::Error::other)?;
    write_model(&mut zip, title, objects)?;
    zip.finish().map_err(io::Error::other)
}

/// The 3MF as bytes.
pub fn to_bytes(title: &str, objects: &[Object]) -> io::Result<Vec<u8>> {
    Ok(write(Cursor::new(Vec::new()), title, objects)?.into_inner())
}

fn write_model<W: Write>(out: &mut W, title: &str, objects: &[Object]) -> io::Result<()> {
    let mut text = String::with_capacity(PIECE + 256);
    text.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    text.push_str(
        "<model unit=\"millimeter\" xml:lang=\"en-US\" \
         xmlns=\"http://schemas.microsoft.com/3dmanufacturing/core/2015/02\">\n",
    );
    let _ = writeln!(text, " <metadata name=\"Title\">{}</metadata>", escape(title));
    let _ = writeln!(text, " <metadata name=\"Application\">Texgrove {}</metadata>", env!("CARGO_PKG_VERSION"));
    text.push_str(" <resources>\n");
    for (i, object) in objects.iter().enumerate() {
        let _ = writeln!(text, "  <object id=\"{}\" type=\"model\" name=\"{}\">", i + 1, escape(object.name));
        text.push_str("   <mesh>\n    <vertices>\n");
        for [x, y, z] in &object.mesh.positions {
            let _ = writeln!(text, "     <vertex x=\"{x}\" y=\"{y}\" z=\"{z}\"/>");
            drain(out, &mut text)?;
        }
        text.push_str("    </vertices>\n    <triangles>\n");
        for [a, b, c] in &object.mesh.triangles {
            let _ = writeln!(text, "     <triangle v1=\"{a}\" v2=\"{b}\" v3=\"{c}\"/>");
            drain(out, &mut text)?;
        }
        text.push_str("    </triangles>\n   </mesh>\n  </object>\n");
    }
    text.push_str(" </resources>\n <build>\n");
    for (i, object) in objects.iter().enumerate() {
        let _ = write!(text, "  <item objectid=\"{}\"", i + 1);
        if let Some(transform) = object.transform {
            text.push_str(" transform=\"");
            for (k, value) in transform.iter().enumerate() {
                let _ = write!(text, "{}{value}", if k == 0 { "" } else { " " });
            }
            text.push('"');
        }
        text.push_str("/>\n");
    }
    text.push_str(" </build>\n</model>\n");
    out.write_all(text.as_bytes())
}

/// Hands the text on once a piece has gathered.
#[inline]
fn drain<W: Write>(out: &mut W, text: &mut String) -> io::Result<()> {
    if text.len() >= PIECE {
        out.write_all(text.as_bytes())?;
        text.clear();
    }
    Ok(())
}

/// Text as it may stand in XML, between tags or in an attribute. Characters XML cannot hold at
/// all (control characters) are left out.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\t' | '\n' | '\r' => out.push(' '),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shapes;
    use std::io::Read;

    fn model_text(bytes: &[u8]) -> String {
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        let names: Vec<String> = archive.file_names().map(|name| name.unwrap().into_owned()).collect();
        assert_eq!(names.len(), 3, "{names:?}");
        assert!(names.iter().any(|n| n == "[Content_Types].xml") && names.iter().any(|n| n == "_rels/.rels"));
        let mut text = String::new();
        archive.by_name("3D/3dmodel.model").unwrap().read_to_string(&mut text).unwrap();
        text
    }

    #[test]
    fn every_part_is_an_object_with_its_name_and_place() {
        let (cube, ball) = (shapes::cuboid([10.0, 20.0, 30.5]), shapes::sphere(5.0, 12, 6));
        let turn = [0.0, 1.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 40.0, 0.0, 2.5];
        let objects = [
            Object { name: "Griff <links> & \"Deckel\"", mesh: &cube, transform: None },
            Object { name: "Kugel", mesh: &ball, transform: Some(turn) },
        ];
        let bytes = to_bytes("Zwei Teile", &objects).unwrap();
        let text = model_text(&bytes);
        assert_eq!(text.matches("<object ").count(), 2);
        assert_eq!(text.matches("<vertex ").count(), 8 + ball.positions.len());
        assert_eq!(text.matches("<triangle ").count(), 12 + ball.triangles.len());
        assert!(text.contains("name=\"Griff &lt;links&gt; &amp; &quot;Deckel&quot;\""));
        assert!(text.contains("<vertex x=\"10\" y=\"20\" z=\"30.5\"/>"));
        assert!(text.contains("<triangle v1=\"0\" v2=\"2\" v3=\"1\"/>"));
        assert!(text.contains("<item objectid=\"1\"/>"));
        assert!(text.contains("<item objectid=\"2\" transform=\"0 1 0 -1 0 0 0 0 1 40 0 2.5\"/>"));
        assert!(text.contains("<metadata name=\"Title\">Zwei Teile</metadata>"));
        assert!(text.contains("unit=\"millimeter\""));
    }

    #[test]
    fn the_same_parts_give_the_same_bytes() {
        let ring = shapes::torus(20.0, 5.0, 64, 32);
        let objects = [Object { name: "Ring", mesh: &ring, transform: None }];
        let first = to_bytes("Ring", &objects).unwrap();
        assert_eq!(first, to_bytes("Ring", &objects).unwrap());
        // Well over one piece of text, so the pieces were joined rightly.
        let text = model_text(&first);
        assert!(text.len() > 3 * PIECE);
        assert!(text.ends_with(" </build>\n</model>\n"));
        assert_eq!(text.matches("<triangle ").count(), ring.triangles.len());
    }

    #[test]
    fn control_characters_stay_out_of_the_xml() {
        assert_eq!(escape("a\u{0}b\tc'd"), "ab c&apos;d");
    }
}
