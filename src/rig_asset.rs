//! Native Bone trees derived from already validated skin bind transforms.
use crate::{Result, skin_import::Binding};
use rbx_dom_weak::{
    InstanceBuilder, WeakDom,
    types::{CFrame, Matrix3, Ref, Variant, Vector3},
};
use std::{collections::HashSet, fs, path::Path};

pub(crate) fn encode(bindings: &[Binding]) -> Result<Vec<u8>> {
    if bindings.is_empty() {
        return Err("skin rig has no bones".into());
    }
    let mut dom = WeakDom::new(InstanceBuilder::new("DataModel"));
    let mut references = Vec::new();
    let mut names = HashSet::new();
    for binding in bindings {
        if binding.name.is_empty()
            || !names.insert(&binding.name)
            || binding.local_bind_cframe.iter().any(|v| !v.is_finite())
        {
            return Err(
                "skin rig requires unique nonempty names and finite bind transforms".into(),
            );
        }
        let parent = match binding.parent {
            Some(index) => *references
                .get(usize::from(index))
                .ok_or("skin rig parent must precede child")?,
            None => dom.root_ref(),
        };
        let c = binding.local_bind_cframe;
        let cframe = CFrame::new(
            Vector3::new(c[9], c[10], c[11]),
            Matrix3::new(
                Vector3::new(c[0], c[1], c[2]),
                Vector3::new(c[3], c[4], c[5]),
                Vector3::new(c[6], c[7], c[8]),
            ),
        );
        let reference = dom.insert(
            parent,
            InstanceBuilder::new("Bone")
                .with_name(&binding.name)
                .with_property("CFrame", cframe),
        );
        references.push(reference);
    }
    let mut bytes = Vec::new();
    rbx_binary::Serializer::new()
        .reflection_database(rbx_reflection_database::get_bundled())
        .serialize(&mut bytes, &dom, dom.root().children())?;
    Ok(bytes)
}

fn bone(dom: &WeakDom, reference: Ref, names: &mut HashSet<String>) -> Result<InstanceBuilder> {
    let node = dom.get_by_ref(reference).ok_or("missing native rig bone")?;
    if node.class.as_str() != "Bone" || node.name.is_empty() || !names.insert(node.name.clone()) {
        return Err("rig artifact requires only uniquely named Bones".into());
    }
    let mut builder = InstanceBuilder::new("Bone").with_name(&node.name);
    for (name, value) in &node.properties {
        if name.as_str() != "CFrame" || !matches!(value, Variant::CFrame(_)) {
            return Err("rig artifact contains unsupported non-bind properties".into());
        }
        builder = builder.with_property(*name, value.clone());
    }
    for child in node.children() {
        builder = builder.with_child(bone(dom, *child, names)?);
    }
    Ok(builder)
}

pub(crate) fn load(path: &Path) -> Result<Vec<InstanceBuilder>> {
    let dom = rbx_binary::from_reader(fs::File::open(path)?)?;
    if dom.root().children().is_empty() {
        return Err("empty native rig artifact".into());
    }
    let mut names = HashSet::new();
    dom.root()
        .children()
        .iter()
        .map(|reference| bone(&dom, *reference, &mut names))
        .collect()
}
