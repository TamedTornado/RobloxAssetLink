pub mod animation;
pub mod animation_fbx;
pub mod animation_gltf;
mod animation_rebase;
pub mod asset_commands;
pub mod audio;
pub mod build_cache;
pub mod bundle;
mod bundle_inputs;
pub mod bundle_verify;
pub mod collision;
mod collision_format;
pub mod convert;
pub mod material;
pub mod material_gltf;
pub mod media_mux;
pub mod media_transcode;
pub mod mesh;
pub mod plan_assets;
mod rig_asset;
pub mod rig_binding;
mod rigid;
pub mod scene;
pub mod scripts;
pub mod skin;
mod skin_fbx;
pub mod skin_import;
mod source_mesh;
pub mod terrain;
pub mod terrain_grid;
pub mod terrain_heightmap;
pub mod terrain_physics;
pub mod terrain_voxels;
pub mod texture;
mod texture_dds;
pub mod texture_pack;
pub mod texture_rgba;
mod vertex_attributes;
pub mod video;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

type Matrix = [[f32; 4]; 4];
const IDENTITY: Matrix = [
    [1., 0., 0., 0.],
    [0., 1., 0., 0.],
    [0., 0., 1., 0.],
    [0., 0., 0., 1.],
];

fn multiply(a: Matrix, b: Matrix) -> Matrix {
    std::array::from_fn(|c| std::array::from_fn(|r| (0..4).map(|k| a[k][r] * b[c][k]).sum()))
}

fn point(m: Matrix, p: [f32; 3], metres_per_stud: f32) -> [f32; 3] {
    std::array::from_fn(|r| {
        (m[0][r] * p[0] + m[1][r] * p[1] + m[2][r] * p[2] + m[3][r]) / metres_per_stud
    })
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

fn normal_matrix(m: Matrix) -> Result<([[f32; 3]; 3], bool)> {
    let columns: [[f32; 3]; 3] = std::array::from_fn(|c| [m[c][0], m[c][1], m[c][2]]);
    let cofactors = [
        cross(columns[1], columns[2]),
        cross(columns[2], columns[0]),
        cross(columns[0], columns[1]),
    ];
    let determinant = dot(columns[0], cofactors[0]);
    if !determinant.is_finite() || determinant == 0. {
        return Err("singular node transform".into());
    }
    Ok((
        cofactors.map(|column| column.map(|value| value / determinant)),
        determinant < 0.,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transforms_use_metres_and_compose_parent_translation() {
        let mut parent = IDENTITY;
        parent[3][0] = 2.8;
        let mut child = IDENTITY;
        child[3][1] = 1.4;
        let p = point(multiply(parent, child), [0., 0., 0.], 0.28);
        assert!((p[0] - 10.).abs() < 0.00001);
        assert!((p[1] - 5.).abs() < 0.00001);
    }

    #[test]
    fn inverse_transpose_and_reflections() {
        let mut matrix = IDENTITY;
        matrix[0][0] = -2.;
        matrix[1][1] = 4.;
        let (normal, mirrored) = normal_matrix(matrix).unwrap();
        assert!(mirrored);
        assert_eq!(normal[0][0], -0.5);
        assert_eq!(normal[1][1], 0.25);
        matrix[0][0] = 0.;
        assert!(normal_matrix(matrix).is_err());
    }
}
