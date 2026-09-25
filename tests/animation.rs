use rbx_dom_weak::types::{CFrame, Variant};
use roblox_asset_link::animation::{Clip, Frame, Marker, Pose, encode};

fn clip() -> Clip {
    Clip {
        name: "Wave".into(),
        looped: true,
        priority: "Action".into(),
        frames: vec![Frame {
            name: "Start".into(),
            time: 0.,
            poses: vec![Pose {
                name: "Root".into(),
                cframe: [1., 0., 0., 0., 1., 0., 0., 0., 1., 0., 0., 0.],
                weight: 1.,
                easing_style: "Linear".into(),
                easing_direction: "In".into(),
                children: vec![],
            }],
            markers: vec![Marker {
                name: "Event".into(),
                value: "value".into(),
            }],
        }],
    }
}

#[test]
fn native_animation_preserves_poses_markers_properties_and_bytes() {
    let source = clip();
    let bytes = encode(&source).unwrap();
    assert_eq!(bytes, encode(&source).unwrap());
    let dom = rbx_binary::from_reader(bytes.as_slice()).unwrap();
    let sequence = dom.get_by_ref(dom.root().children()[0]).unwrap();
    assert_eq!(sequence.class.as_str(), "KeyframeSequence");
    assert_eq!(sequence.name, "Wave");
    assert_eq!(
        sequence.properties.get(&"Loop".into()),
        Some(&Variant::Bool(true))
    );
    let frame = dom.get_by_ref(sequence.children()[0]).unwrap();
    assert_eq!(
        frame.properties.get(&"Time".into()),
        Some(&Variant::Float32(0.))
    );
    let pose = dom.get_by_ref(frame.children()[0]).unwrap();
    assert_eq!(pose.name, "Root");
    assert_eq!(
        pose.properties.get(&"CFrame".into()),
        Some(&Variant::CFrame(CFrame::identity()))
    );
    let marker = dom.get_by_ref(frame.children()[1]).unwrap();
    assert_eq!(marker.class.as_str(), "KeyframeMarker");
    assert_eq!(
        marker.properties.get(&"Value".into()),
        Some(&Variant::String("value".into()))
    );
}

#[test]
fn invalid_times_enums_and_pose_data_fail() {
    let mut source = clip();
    source.frames[0].time = f32::NAN;
    assert!(encode(&source).is_err());
    source = clip();
    source.priority = "Imaginary".into();
    assert!(encode(&source).is_err());
    source = clip();
    source.frames[0].poses[0].weight = 2.;
    assert!(encode(&source).is_err());
    source = clip();
    source.frames.push(clip().frames.remove(0));
    assert!(encode(&source).is_err());
}
