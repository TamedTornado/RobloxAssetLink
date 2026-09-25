use roblox_asset_link::{
    terrain_grid,
    terrain_physics::{Grid, Limits, decode, encode},
};
use std::{collections::HashSet, fs};

#[test]
fn independent_physics_grid_reencodes_exactly_and_its_fixture_coverage_matches_voxel_halo() {
    let dom = rbx_binary::from_reader(fs::File::open("tests/fixtures/terrain-rojo.rbxm").unwrap())
        .unwrap();
    let terrain = dom.get_by_ref(dom.root().children()[0]).unwrap();
    let property = |name: &str| -> &[u8] {
        let rbx_dom_weak::types::Variant::BinaryString(bytes) = &terrain.properties[&name.into()]
        else {
            panic!("missing {name}");
        };
        bytes.as_ref()
    };
    let bytes = property("PhysicsGrid");
    let limits = Limits { max_entries: 194 };
    let physics = decode(bytes, &limits).unwrap();
    assert_eq!(bytes.len(), 2342);
    assert_eq!(physics.exponent, 3);
    assert_eq!(
        physics
            .coordinate_groups
            .each_ref()
            .map(|group| group.len()),
        [194, 0, 0]
    );
    assert_eq!(encode(&physics, &limits).unwrap(), bytes);
    let actual: HashSet<_> = physics.coordinate_groups[0].iter().copied().collect();
    assert_eq!(actual.len(), 136); // Do not discard the fixture's repeated entries.

    let smooth = terrain_grid::decode(
        property("SmoothGrid"),
        &terrain_grid::Limits {
            max_chunks: 16,
            max_cells: 524288,
        },
    )
    .unwrap();
    let mut expected = HashSet::new();
    for chunk in smooth.chunks {
        for (index, cell) in chunk.cells.iter().enumerate() {
            if cell.material == 0 {
                continue;
            }
            let local = [index % 32, index / (32 * 32), (index / 32) % 32];
            let position: [i32; 3] =
                std::array::from_fn(|axis| chunk.coordinate[axis] * 32 + local[axis] as i32);
            for x in -1..=1 {
                for y in -1..=1 {
                    for z in -1..=1 {
                        expected.insert([
                            (position[0] + x).div_euclid(8),
                            (position[1] + y).div_euclid(8),
                            (position[2] + z).div_euclid(8),
                        ]);
                    }
                }
            }
        }
    }
    // An observed fixture relationship, not a claimed general collision cooker.
    assert_eq!(actual, expected);
}

#[test]
fn groups_reset_coordinate_deltas_and_preserve_duplicates_and_wrapping() {
    let grid = Grid {
        exponent: 3,
        coordinate_groups: [
            vec![[i32::MIN, -1, i32::MAX], [i32::MAX, 0, i32::MIN]],
            vec![[-1, 2, 3], [-1, 2, 3]],
            vec![[0, -2, 5]],
        ],
    };
    let limits = Limits { max_entries: 5 };
    let bytes = encode(&grid, &limits).unwrap();
    assert_eq!(decode(&bytes, &limits).unwrap(), grid);
    assert!(decode(&bytes, &Limits { max_entries: 4 }).is_err());
    for end in 0..bytes.len() {
        assert!(decode(&bytes[..end], &limits).is_err());
    }
    let mut trailing = bytes;
    trailing.push(0);
    assert!(decode(&trailing, &limits).is_err());
    assert!(decode(&[1, 3], &limits).is_err());
    assert!(decode(&[2, 9], &limits).is_err());
}
