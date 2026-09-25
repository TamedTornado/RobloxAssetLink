//! Offline native model/place serialization from explicitly typed scene data.
use crate::Result;
use rbx_dom_weak::{
    InstanceBuilder, WeakDom,
    types::{Ref, Variant},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap},
    fs,
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Specification {
    pub kind: Kind,
    pub roots: Vec<Node>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    Model,
    Place,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Node {
    pub id: String,
    pub class: String,
    pub name: String,
    pub properties: BTreeMap<String, Variant>,
    pub references: BTreeMap<String, String>,
    pub children: Vec<Node>,
    pub script_source: Option<PathBuf>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildResult {
    pub format: &'static str,
    pub sha256: String,
    pub instances: usize,
    pub bytes: usize,
    pub published: bool,
}

fn insert(
    dom: &mut WeakDom,
    parent: Ref,
    node: &Node,
    root: &Path,
    ids: &mut HashMap<String, Ref>,
) -> Result<()> {
    if node.id.is_empty() || node.class.is_empty() {
        return Err("scene node id and class must be nonempty".into());
    }
    let database = rbx_reflection_database::get_bundled();
    if !database.classes.contains_key(node.class.as_str()) {
        return Err(format!("unknown scene class: {}", node.class).into());
    }
    for property in node.properties.keys().chain(node.references.keys()) {
        let mut class = Some(node.class.as_str());
        let mut found = false;
        while let Some(name) = class {
            let descriptor = database
                .classes
                .get(name)
                .ok_or("reflection superclass missing")?;
            if descriptor.properties.contains_key(property.as_str()) {
                found = true;
                break;
            }
            class = descriptor.superclass;
        }
        if !found {
            return Err(format!("unknown property {}.{property}", node.class).into());
        }
    }
    if ids.contains_key(&node.id) {
        return Err(format!("duplicate scene id: {}", node.id).into());
    }
    if node.class == "DataModel" {
        return Err("DataModel is the implicit root; do not nest it".into());
    }
    let mut builder = InstanceBuilder::new(node.class.as_str()).with_name(&node.name);
    for (name, value) in &node.properties {
        if name == "Name"
            || name == "Parent"
            || matches!(value, Variant::Ref(_))
            || node.references.contains_key(name)
        {
            return Err(format!(
                "property {name} conflicts with explicit scene structure/references"
            )
            .into());
        }
        builder = builder.with_property(name.as_str(), value.clone());
    }
    if let Some(source) = &node.script_source {
        if !matches!(
            node.class.as_str(),
            "Script" | "LocalScript" | "ModuleScript"
        ) || node.properties.contains_key("Source")
        {
            return Err(
                "scriptSource requires a script class without an inline Source property".into(),
            );
        }
        let path = root.join(source).canonicalize()?;
        if source.is_absolute() || !path.starts_with(root) {
            return Err("scriptSource must remain under the scene source directory".into());
        }
        builder = builder.with_property("Source", fs::read_to_string(path)?);
    }
    let reference = dom.insert(parent, builder);
    ids.insert(node.id.clone(), reference);
    for child in &node.children {
        insert(dom, reference, child, root, ids)?;
    }
    Ok(())
}

fn link(dom: &mut WeakDom, node: &Node, ids: &HashMap<String, Ref>) -> Result<()> {
    let reference = ids[&node.id];
    for (property, target) in &node.references {
        if property == "Name" || property == "Parent" {
            return Err("references cannot override Name or Parent".into());
        }
        let target = *ids
            .get(target)
            .ok_or_else(|| format!("missing scene reference: {target}"))?;
        dom.get_by_ref_mut(reference)
            .ok_or("scene instance vanished")?
            .properties
            .insert(property.as_str().into(), Variant::Ref(target));
    }
    for child in &node.children {
        link(dom, child, ids)?;
    }
    Ok(())
}

pub fn build(source: &Path, output: &Path) -> Result<BuildResult> {
    let source = source.canonicalize()?;
    let root = source.parent().ok_or("scene source directory missing")?;
    let specification: Specification = serde_json::from_slice(&fs::read(&source)?)?;
    if specification.roots.is_empty() {
        return Err("scene needs at least one root instance".into());
    }
    let format = match specification.kind {
        Kind::Model => "rbxm",
        Kind::Place => "rbxl",
    };
    if output.extension().and_then(|v| v.to_str()) != Some(format) {
        return Err(format!("output extension must be .{format} for this scene kind").into());
    }
    let mut dom = WeakDom::new(InstanceBuilder::new("DataModel"));
    let mut ids = HashMap::new();
    let parent = dom.root_ref();
    for node in &specification.roots {
        insert(&mut dom, parent, node, root, &mut ids)?;
    }
    for node in &specification.roots {
        link(&mut dom, node, &ids)?;
    }
    let mut bytes = Vec::new();
    // Cargo enables the library's always-bundled feature, including its constructor.
    // Neither RBX_DATABASE nor a user's local reflection cache affects this build.
    rbx_binary::Serializer::new()
        .reflection_database(rbx_reflection_database::get_bundled())
        .serialize(&mut bytes, &dom, dom.root().children())?;
    let result = BuildResult {
        format,
        sha256: format!("{:x}", Sha256::digest(&bytes)),
        instances: ids.len(),
        bytes: bytes.len(),
        published: false,
    };
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(&bytes)?;
    temporary.as_file().sync_all()?;
    temporary.persist_noclobber(output)?;
    Ok(result)
}
