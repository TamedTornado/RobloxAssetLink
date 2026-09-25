//! Unlayered SurfaceAppearance TexturePack v2 descriptor, not pixel compression.
use crate::Result;
use std::collections::BTreeMap;

/// Read the canonical local profile we emit, not arbitrary Roblox XML documents.
pub fn references(bytes: &[u8]) -> Result<BTreeMap<String, String>> {
    let text = std::str::from_utf8(bytes)?;
    let prefix = "<roblox>\n  <texturepack_version>2</texturepack_version>\n  <usage>0</usage>\n  <alphamode>";
    let rest = text
        .strip_prefix(prefix)
        .ok_or("unsupported TexturePack profile")?;
    let (alpha, rest) = rest
        .split_once("</alphamode>\n  <tiling>0</tiling>\n")
        .ok_or("invalid TexturePack header")?;
    let body = rest
        .strip_suffix("</roblox>")
        .ok_or("invalid TexturePack ending")?;
    let mut maps = BTreeMap::new();
    for line in body.lines() {
        let line = line
            .strip_prefix("  <")
            .ok_or("invalid TexturePack channel")?;
        let (channel, rest) = line.split_once('>').ok_or("invalid TexturePack channel")?;
        let uri = rest
            .strip_suffix(&format!("</{channel}>"))
            .ok_or("invalid TexturePack closing tag")?;
        if maps.insert(channel.to_owned(), uri.to_owned()).is_some() {
            return Err("duplicate TexturePack channel".into());
        }
    }
    if encode(alpha.parse()?, &maps)? != bytes {
        return Err("noncanonical local TexturePack descriptor".into());
    }
    Ok(maps)
}

pub fn encode(alpha_mode: u32, maps: &BTreeMap<String, String>) -> Result<Vec<u8>> {
    if alpha_mode > 3 {
        return Err("TexturePack alpha mode is outside the native v2 range".into());
    }
    const CHANNELS: [&str; 6] = [
        "color",
        "normal",
        "metalness",
        "roughness",
        "emissive",
        "height",
    ];
    for (channel, uri) in maps {
        if !CHANNELS.contains(&channel.as_str()) {
            return Err("unknown TexturePack channel".into());
        }
        let path = uri
            .strip_prefix("rbxasset://")
            .ok_or("TexturePack requires local content URIs")?;
        if path.is_empty()
            || path.split('/').any(|segment| {
                segment.is_empty()
                    || segment == "."
                    || segment == ".."
                    || !segment
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
            })
        {
            return Err("TexturePack requires safe contained local content paths".into());
        }
    }
    // Usage 0 and tiling 0 are fixed SurfaceAppearance protocol values from the
    // native builder, not project defaults or guessed enum assignments.
    let mut xml = format!(
        "<roblox>\n  <texturepack_version>2</texturepack_version>\n  <usage>0</usage>\n  <alphamode>{alpha_mode}</alphamode>\n  <tiling>0</tiling>\n"
    );
    for channel in CHANNELS {
        if let Some(uri) = maps.get(channel) {
            xml.push_str(&format!("  <{channel}>{uri}</{channel}>\n"));
        }
    }
    xml.push_str("</roblox>");
    Ok(xml.into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptor_matches_inspected_native_writer_layout() {
        let maps = BTreeMap::from([
            ("roughness".into(), "rbxasset://kit/r.dds".into()),
            ("color".into(), "rbxasset://kit/c.png".into()),
        ]);
        let bytes = encode(1, &maps).unwrap();
        assert_eq!(
            String::from_utf8(bytes.clone()).unwrap(),
            "<roblox>\n  <texturepack_version>2</texturepack_version>\n  <usage>0</usage>\n  <alphamode>1</alphamode>\n  <tiling>0</tiling>\n  <color>rbxasset://kit/c.png</color>\n  <roughness>rbxasset://kit/r.dds</roughness>\n</roblox>"
        );
        assert_eq!(encode(1, &maps).unwrap(), bytes);
        assert_eq!(references(&bytes).unwrap(), maps);
        assert!(references(b"<roblox><!DOCTYPE evil></roblox>").is_err());
        assert!(encode(4, &maps).is_err());
        for uri in [
            "rbxasset://../bad",
            "rbxasset://x?a&b",
            "https://host/image",
            "rbxasset://x/<tag>",
        ] {
            assert!(encode(0, &BTreeMap::from([("color".into(), uri.into())])).is_err());
        }
    }
}
