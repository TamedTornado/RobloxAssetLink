//! Native Roblox mesh v2.00 encoding. No engine, network or external executable.
//!
//! Layout reference: krakow10/rbx_mesh, src/mesh/v2.rs (MIT OR Apache-2.0).
//! The four bytes after UV are packed tangent data, NOT a third UV float.

use crate::Result;

const SIGNATURE: &[u8] = b"version 2.00\n";
const HEADER_BYTES: u16 = 12;
const VERTEX_BYTES: u8 = 40;
const FACE_BYTES: u8 = 12;

#[derive(Clone, Debug, PartialEq)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    /// Packed biased bytes: component = (byte - 127) / 127.
    /// All zero is the historical missing-tangent marker.
    pub tangent: [u8; 4],
    pub color: [u8; 4],
}

#[derive(Clone, Debug, PartialEq)]
pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub triangles: Vec<[u32; 3]>,
}

/// Axis-aligned bounds in the encoded mesh coordinate space, in studs.
#[derive(Clone, Debug, serde::Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Bounds {
    pub min: [f32; 3],
    pub max: [f32; 3],
    pub center: [f32; 3],
    pub size: [f32; 3],
}

impl Mesh {
    pub fn bounds(&self) -> Result<Bounds> {
        bounds(self.vertices.iter())
    }

    /// v4 subset encoding writes only vertices referenced by triangles.
    pub fn surface_bounds(&self) -> Result<Bounds> {
        if self
            .triangles
            .iter()
            .flatten()
            .any(|index| *index as usize >= self.vertices.len())
        {
            return Err("mesh bounds triangle index out of range".into());
        }
        bounds(
            self.triangles
                .iter()
                .flatten()
                .map(|index| &self.vertices[*index as usize]),
        )
    }
}

fn bounds<'a>(mut vertices: impl Iterator<Item = &'a Vertex>) -> Result<Bounds> {
    let first = vertices.next().ok_or("mesh bounds require vertices")?;
    let mut min = first.position;
    let mut max = first.position;
    for vertex in std::iter::once(first).chain(vertices) {
        for axis in 0..3 {
            let value = vertex.position[axis];
            if !value.is_finite() {
                return Err("mesh bounds require finite positions".into());
            }
            min[axis] = min[axis].min(value);
            max[axis] = max[axis].max(value);
        }
    }
    let size: [f32; 3] = std::array::from_fn(|axis| max[axis] - min[axis]);
    if size.iter().any(|value| !value.is_finite()) {
        return Err("mesh extent exceeds native float representation".into());
    }
    let center =
        std::array::from_fn(|axis| ((f64::from(min[axis]) + f64::from(max[axis])) * 0.5) as f32);
    Ok(Bounds {
        min,
        max,
        center,
        size,
    })
}

pub fn encode(mesh: &Mesh) -> Result<Vec<u8>> {
    let vertices = u32::try_from(mesh.vertices.len())?;
    let faces = u32::try_from(mesh.triangles.len())?;
    if vertices == 0 || faces == 0 {
        return Err("mesh requires vertices and triangles".into());
    }
    for vertex in &mesh.vertices {
        if vertex
            .position
            .iter()
            .chain(&vertex.normal)
            .chain(&vertex.uv)
            .any(|value| !value.is_finite())
            || vertex.normal.iter().all(|value| *value == 0.)
        {
            return Err("mesh attributes must be finite with a nonzero normal".into());
        }
    }
    if mesh
        .triangles
        .iter()
        .flatten()
        .any(|index| *index >= vertices)
    {
        return Err("mesh triangle index out of range".into());
    }

    let mut bytes = Vec::new();
    bytes.extend_from_slice(SIGNATURE);
    bytes.extend_from_slice(&HEADER_BYTES.to_le_bytes());
    bytes.extend_from_slice(&[VERTEX_BYTES, FACE_BYTES]);
    bytes.extend_from_slice(&vertices.to_le_bytes());
    bytes.extend_from_slice(&faces.to_le_bytes());
    for vertex in &mesh.vertices {
        for value in vertex
            .position
            .iter()
            .chain(&vertex.normal)
            .chain(&vertex.uv)
        {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes.extend(vertex.tangent);
        bytes.extend(vertex.color);
    }
    for index in mesh.triangles.iter().flatten() {
        bytes.extend_from_slice(&index.to_le_bytes());
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn triangle() -> Mesh {
        Mesh {
            vertices: vec![
                Vertex {
                    position: [1., 2., 3.],
                    normal: [0., 1., 0.],
                    uv: [0.25, 0.75],
                    tangent: [127, 127, 0, 254],
                    color: [1, 2, 3, 4],
                };
                3
            ],
            triangles: vec![[0, 1, 2]],
        }
    }

    #[test]
    fn exact_header_attributes_and_indices() {
        let bytes = encode(&triangle()).unwrap();
        assert_eq!(
            &bytes[..25],
            b"version 2.00\n\x0c\0\x28\x0c\x03\0\0\0\x01\0\0\0"
        );
        assert_eq!(bytes.len(), 25 + 3 * 40 + 12);
        assert_eq!(&bytes[25..29], &1_f32.to_le_bytes());
        assert_eq!(&bytes[49..53], &0.25_f32.to_le_bytes());
        assert_eq!(&bytes[53..57], &0.75_f32.to_le_bytes());
        assert_eq!(&bytes[57..65], &[127, 127, 0, 254, 1, 2, 3, 4]);
        assert_eq!(&bytes[145..], &[0, 0, 0, 0, 1, 0, 0, 0, 2, 0, 0, 0]);
        assert_eq!(bytes, encode(&triangle()).unwrap());
    }

    #[test]
    fn rejects_invalid_geometry() {
        let mut mesh = triangle();
        mesh.vertices[0].uv[0] = f32::NAN;
        assert!(encode(&mesh).is_err());
        mesh = triangle();
        mesh.vertices[0].normal = [0.; 3];
        assert!(encode(&mesh).is_err());
        mesh = triangle();
        mesh.triangles[0][2] = 3;
        assert!(encode(&mesh).is_err());
        mesh = triangle();
        mesh.triangles.clear();
        assert!(encode(&mesh).is_err());
    }
}
