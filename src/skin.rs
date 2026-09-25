//! Native v4.01 skinned mesh writer. Input adapters must supply bind-space data.
use crate::{Result, mesh};
use std::collections::{BTreeMap, BTreeSet, HashSet};

/// Fixed by the native v4 subset record, not an execution policy.
const PALETTE_BONES: usize = 26;
const NO_BONE: u16 = u16::MAX;

pub struct Bone {
    pub name: String,
    pub parent: Option<u16>,
    /// World bind pose: row-major rotation followed by translation, in studs.
    pub bind_cframe: [f32; 12],
    pub cull_distance: f32,
}

#[derive(Clone)]
pub struct Influences {
    pub joints: [u16; 4],
    pub weights: [f32; 4],
}

struct Subset {
    face_start: u32,
    vertex_start: u32,
    palette: BTreeSet<u16>,
    vertices: BTreeMap<u32, u32>,
    faces: Vec<[u32; 3]>,
}

fn weights(input: [f32; 4]) -> Result<[u8; 4]> {
    if input.iter().any(|v| !v.is_finite() || *v < 0.) {
        return Err("skin weights must be finite and nonnegative".into());
    }
    let sum: f64 = input.iter().map(|v| f64::from(*v)).sum();
    if sum == 0. {
        return Err("skin vertex has no positive weights".into());
    }
    let scaled = input.map(|v| f64::from(v) / sum * 255.);
    let mut result = scaled.map(|v| v.floor() as u8);
    let remaining = 255 - result.iter().map(|v| u16::from(*v)).sum::<u16>();
    let mut order = [0, 1, 2, 3];
    order.sort_by(|&a, &b| {
        (scaled[b] - scaled[b].floor())
            .total_cmp(&(scaled[a] - scaled[a].floor()))
            .then(a.cmp(&b))
    });
    for index in order.into_iter().take(usize::from(remaining)) {
        result[index] += 1;
    }
    Ok(result)
}

pub fn encode(geometry: &mesh::Mesh, influences: &[Influences], bones: &[Bone]) -> Result<Vec<u8>> {
    mesh::encode(geometry)?;
    if bones.is_empty()
        || bones.len() > usize::from(NO_BONE)
        || influences.len() != geometry.vertices.len()
    {
        return Err("skinned mesh needs a valid skeleton and one envelope per vertex".into());
    }
    let mut names = HashSet::new();
    for (index, bone) in bones.iter().enumerate() {
        if bone.name.is_empty()
            || bone.name.contains('\0')
            || !names.insert(&bone.name)
            || bone
                .parent
                .is_some_and(|parent| usize::from(parent) >= index)
            || bone.bind_cframe.iter().any(|v| !v.is_finite())
            || !bone.cull_distance.is_finite()
            || bone.cull_distance < 0.
        {
            return Err("invalid bone name, parent ordering, bind pose or cull distance".into());
        }
    }
    let packed: Vec<_> = influences
        .iter()
        .map(|influence| -> Result<_> {
            for (joint, weight) in influence.joints.iter().zip(influence.weights) {
                if weight > 0. && usize::from(*joint) >= bones.len() {
                    return Err("skin joint index out of range".into());
                }
            }
            weights(influence.weights)
        })
        .collect::<Result<_>>()?;

    let mut subsets = vec![Subset {
        face_start: 0,
        vertex_start: 0,
        palette: BTreeSet::new(),
        vertices: BTreeMap::new(),
        faces: Vec::new(),
    }];
    for triangle in &geometry.triangles {
        let used: BTreeSet<_> = triangle
            .iter()
            .flat_map(|i| {
                influences[*i as usize]
                    .joints
                    .into_iter()
                    .zip(packed[*i as usize])
            })
            .filter_map(|(joint, weight)| (weight > 0).then_some(joint))
            .collect();
        let current = subsets.last().ok_or("missing skin subset")?;
        if current.palette.union(&used).count() > PALETTE_BONES {
            subsets.push(Subset {
                face_start: 0,
                vertex_start: 0,
                palette: BTreeSet::new(),
                vertices: BTreeMap::new(),
                faces: Vec::new(),
            });
        }
        let current = subsets.last_mut().ok_or("missing skin subset")?;
        current.palette.extend(used);
        for vertex in triangle {
            let index = u32::try_from(current.vertices.len())?;
            current.vertices.entry(*vertex).or_insert(index);
        }
        current.faces.push(*triangle);
    }
    let mut vertices = Vec::new();
    let mut envelopes = Vec::new();
    let mut faces = Vec::new();
    for subset in &mut subsets {
        subset.face_start = u32::try_from(faces.len())?;
        subset.vertex_start = u32::try_from(vertices.len())?;
        let palette: BTreeMap<_, _> = subset
            .palette
            .iter()
            .enumerate()
            .map(|(i, bone)| (*bone, i as u8))
            .collect();
        let mut local: Vec<_> = subset.vertices.iter().collect();
        local.sort_by_key(|(_, index)| **index);
        for (&original, _) in local {
            vertices.push(geometry.vertices[original as usize].clone());
            let joints: [u8; 4] = std::array::from_fn(|i| {
                if packed[original as usize][i] == 0 {
                    0
                } else {
                    palette[&influences[original as usize].joints[i]]
                }
            });
            envelopes.push((joints, packed[original as usize]));
        }
        for face in &subset.faces {
            faces.push(face.map(|i| subset.vertex_start + subset.vertices[&i]));
        }
    }
    let flattened = mesh::Mesh {
        vertices,
        triangles: faces,
    };
    let static_bytes = mesh::encode(&flattened)?;
    let mut bone_names = Vec::new();
    let mut bone_offsets = Vec::new();
    for bone in bones {
        bone_offsets.push(u32::try_from(bone_names.len())?);
        bone_names.extend_from_slice(bone.name.as_bytes());
        bone_names.push(0);
    }

    let mut bytes = b"version 4.01\n".to_vec();
    bytes.extend_from_slice(&24_u16.to_le_bytes()); // native header size
    bytes.extend_from_slice(&0_u16.to_le_bytes()); // no simplification algorithm
    bytes.extend_from_slice(&u32::try_from(flattened.vertices.len())?.to_le_bytes());
    bytes.extend_from_slice(&u32::try_from(flattened.triangles.len())?.to_le_bytes());
    bytes.extend_from_slice(&2_u16.to_le_bytes()); // two offsets delimit the single LOD
    bytes.extend_from_slice(&u16::try_from(bones.len())?.to_le_bytes());
    bytes.extend_from_slice(&u32::try_from(bone_names.len())?.to_le_bytes());
    bytes.extend_from_slice(&u16::try_from(subsets.len())?.to_le_bytes());
    bytes.extend_from_slice(&[1, 0]); // one high-quality LOD, padding
    let faces_offset = 25 + flattened.vertices.len() * 40;
    bytes.extend_from_slice(&static_bytes[25..faces_offset]);
    for (joints, weights) in envelopes {
        bytes.extend(joints);
        bytes.extend(weights);
    }
    bytes.extend_from_slice(&static_bytes[faces_offset..]);
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    bytes.extend_from_slice(&u32::try_from(flattened.triangles.len())?.to_le_bytes());
    for (bone, offset) in bones.iter().zip(bone_offsets) {
        bytes.extend_from_slice(&offset.to_le_bytes());
        bytes.extend_from_slice(&bone.parent.unwrap_or(NO_BONE).to_le_bytes());
        bytes.extend_from_slice(&NO_BONE.to_le_bytes());
        bytes.extend_from_slice(&bone.cull_distance.to_le_bytes());
        for value in bone.bind_cframe {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
    }
    bytes.extend(bone_names);
    for subset in subsets {
        for value in [
            subset.face_start,
            u32::try_from(subset.faces.len())?,
            subset.vertex_start,
            u32::try_from(subset.vertices.len())?,
            u32::try_from(subset.palette.len())?,
        ] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        for bone in subset
            .palette
            .iter()
            .copied()
            .chain(std::iter::repeat(NO_BONE))
            .take(PALETTE_BONES)
        {
            bytes.extend_from_slice(&bone.to_le_bytes());
        }
    }
    Ok(bytes)
}
