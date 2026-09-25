//! Local convex cooking and documented CSGPHS v5 encoding.
//! Format conformity is not proof of current MeshPart engine acceptance.

pub use crate::collision_format::encode;
use crate::{Result, mesh};
use parry3d::{
    math::Vector,
    transformation::{
        try_convex_hull,
        vhacd::{VHACD, VHACDParameters},
        voxelization::FillMode,
    },
};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(tag = "mode", rename_all = "camelCase", deny_unknown_fields)]
pub enum Recipe {
    Box,
    Hull,
    Decomposition {
        concavity: f32,
        alpha: f32,
        beta: f32,
        resolution: u32,
        #[serde(rename = "planeDownsampling")]
        plane_downsampling: u32,
        #[serde(rename = "convexHullDownsampling")]
        convex_hull_downsampling: u32,
        #[serde(rename = "maxConvexHulls")]
        max_convex_hulls: u32,
        #[serde(rename = "detectCavities")]
        detect_cavities: bool,
        #[serde(rename = "convexHullApproximation")]
        convex_hull_approximation: bool,
    },
}

#[derive(Debug)]
pub struct Hull {
    pub positions: Vec<[f32; 3]>,
    pub triangles: Vec<[u32; 3]>,
}

pub fn cook(mesh: &mesh::Mesh, recipe: &Recipe) -> Result<Vec<Hull>> {
    // Reuse native geometry validation, including finite values and indices.
    mesh::encode(mesh)?;
    let positions: Vec<_> = mesh
        .vertices
        .iter()
        .map(|v| Vector::from_array(v.position))
        .collect();
    let mut minimum = [f32::INFINITY; 3];
    let mut maximum = [f32::NEG_INFINITY; 3];
    for vertex in &mesh.vertices {
        for axis in 0..3 {
            minimum[axis] = minimum[axis].min(vertex.position[axis]);
            maximum[axis] = maximum[axis].max(vertex.position[axis]);
        }
    }
    if (0..3).any(|axis| minimum[axis] >= maximum[axis]) {
        return Err("collision geometry must have nonzero extent on every axis".into());
    }
    let raw = match recipe {
        Recipe::Box => {
            let corners: Vec<_> = (0..8)
                .map(|index| {
                    Vector::from_array(std::array::from_fn(|axis| {
                        if index & (1 << axis) == 0 {
                            minimum[axis]
                        } else {
                            maximum[axis]
                        }
                    }))
                })
                .collect();
            vec![try_convex_hull(&corners).map_err(|e| format!("box hull failed: {e:?}"))?]
        }
        Recipe::Hull => {
            vec![try_convex_hull(&positions).map_err(|e| format!("convex hull failed: {e:?}"))?]
        }
        Recipe::Decomposition {
            concavity,
            alpha,
            beta,
            resolution,
            plane_downsampling,
            convex_hull_downsampling,
            max_convex_hulls,
            detect_cavities,
            convex_hull_approximation,
        } => {
            if [*concavity, *alpha, *beta]
                .iter()
                .any(|v| !v.is_finite() || !(0. ..=1.).contains(v))
                || *resolution < 2
                || resolution.checked_add(2).is_none()
                || *plane_downsampling == 0
                || *convex_hull_downsampling == 0
                || *max_convex_hulls == 0
            {
                return Err("invalid decomposition configuration: ratios must be in [0,1], resolution at least two, and counts positive".into());
            }
            let params = VHACDParameters {
                concavity: *concavity,
                alpha: *alpha,
                beta: *beta,
                resolution: *resolution,
                plane_downsampling: *plane_downsampling,
                convex_hull_downsampling: *convex_hull_downsampling,
                max_convex_hulls: *max_convex_hulls,
                convex_hull_approximation: *convex_hull_approximation,
                fill_mode: FillMode::FloodFill {
                    detect_cavities: *detect_cavities,
                },
            };
            VHACD::decompose(&params, &positions, &mesh.triangles, true)
                .compute_exact_convex_hulls(&positions, &mesh.triangles)
        }
    };
    if raw.is_empty() {
        return Err("collision decomposition produced no hulls".into());
    }
    if let Recipe::Decomposition {
        max_convex_hulls, ..
    } = recipe
        && raw.len() > *max_convex_hulls as usize
    {
        // VHACD's split budget is not a guarantee on exact output hull count.
        // Never quietly exceed the caller's budget or merge away collision detail.
        return Err(format!("decomposition produced {} hulls, exceeding configured maxConvexHulls {}; use hull mode or adjust the recipe", raw.len(), max_convex_hulls).into());
    }
    Ok(raw
        .into_iter()
        .map(|(positions, triangles)| Hull {
            positions: positions.into_iter().map(|p| p.to_array()).collect(),
            triangles,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tetrahedron() -> mesh::Mesh {
        mesh::Mesh {
            vertices: [[0., 0., 0.], [2., 0., 0.], [0., 2., 0.], [0., 0., 2.]]
                .into_iter()
                .map(|position| mesh::Vertex {
                    position,
                    normal: [0., 1., 0.],
                    uv: [0.; 2],
                    tangent: [0; 4],
                    color: [255; 4],
                })
                .collect(),
            triangles: vec![[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]],
        }
    }

    #[test]
    fn box_and_hull_have_distinct_geometry_and_decode_independently() {
        for (recipe, expected_vertices) in [(Recipe::Box, 8), (Recipe::Hull, 4)] {
            let hulls = cook(&tetrahedron(), &recipe).unwrap();
            assert_eq!(hulls.len(), 1);
            assert_eq!(hulls[0].positions.len(), expected_vertices);
            let bytes = encode(&hulls).unwrap();
            assert_eq!(&bytes[..10], b"CSGPHS\x05\0\0\0");
            let mut cursor = std::io::Cursor::new(&bytes);
            let decoded = rbx_mesh::read_union_physics_versioned(&mut cursor).unwrap();
            assert_eq!(cursor.position() as usize, bytes.len());
            let rbx_mesh::union_physics::UnionPhysics::V5(decoded) = decoded else {
                panic!("wrong collision version");
            };
            assert_eq!(decoded.meshes.len(), 1);
            assert_eq!(decoded.meshes[0].positions, hulls[0].positions);
            assert_eq!(decoded.meshes[0].faces.len(), hulls[0].triangles.len());
            assert_eq!(
                bytes,
                encode(&cook(&tetrahedron(), &recipe).unwrap()).unwrap()
            );
        }
    }

    #[test]
    fn decomposition_requires_explicit_valid_policy_and_is_repeatable() {
        let json = r#"{"mode":"decomposition","concavity":0.01,"alpha":0.05,"beta":0.05,"resolution":12,"planeDownsampling":2,"convexHullDownsampling":2,"maxConvexHulls":64,"detectCavities":false,"convexHullApproximation":false}"#;
        let recipe: Recipe = serde_json::from_str(json).unwrap();
        let first = encode(&cook(&tetrahedron(), &recipe).unwrap()).unwrap();
        let second = encode(&cook(&tetrahedron(), &recipe).unwrap()).unwrap();
        assert_eq!(first, second);
        assert!(serde_json::from_str::<Recipe>(r#"{"mode":"decomposition"}"#).is_err());
        for resolution in [0, 1] {
            let mut value: serde_json::Value = serde_json::from_str(json).unwrap();
            value["resolution"] = resolution.into();
            assert!(cook(&tetrahedron(), &serde_json::from_value(value).unwrap()).is_err());
        }
    }

    #[test]
    fn malformed_and_flat_meshes_are_rejected() {
        let mut source = tetrahedron();
        source.vertices[0].position[0] = f32::NAN;
        assert!(cook(&source, &Recipe::Hull).is_err());
        source = tetrahedron();
        for vertex in &mut source.vertices {
            vertex.position[2] = 0.;
        }
        assert!(cook(&source, &Recipe::Box).is_err());
        assert!(encode(&[]).is_err());
    }

    #[test]
    fn disconnected_geometry_respects_configured_hull_budget() {
        let mut source = tetrahedron();
        source
            .vertices
            .extend(tetrahedron().vertices.into_iter().map(|mut vertex| {
                vertex.position[0] += 6.;
                vertex
            }));
        source.triangles.extend(
            tetrahedron()
                .triangles
                .into_iter()
                .map(|face| face.map(|i| i + 4)),
        );
        let recipe = |max_convex_hulls| Recipe::Decomposition {
            concavity: 0.05,
            alpha: 0.,
            beta: 0.,
            resolution: 24,
            plane_downsampling: 1,
            convex_hull_downsampling: 1,
            max_convex_hulls,
            detect_cavities: false,
            convex_hull_approximation: false,
        };
        assert!(
            cook(&source, &recipe(1))
                .err()
                .unwrap()
                .to_string()
                .contains("exceeding configured maxConvexHulls")
        );
        let hulls = cook(&source, &recipe(64)).unwrap();
        assert!(hulls.len() > 1 && hulls.len() <= 64);
    }
}
