// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2025-2026 Sreehari Anil and project contributors

use presenceforge::{
    Activity, ActivityAssets, ActivityBuilder, ActivityParty, ActivityType, StatusDisplayType,
};
use serde_json::Value;

fn activity_to_value(activity: &Activity) -> Value {
    serde_json::to_value(activity).expect("activity should serialize")
}

#[test]
fn builder_produces_serializable_activity() {
    let activity = ActivityBuilder::new()
        .state("Playing campaign")
        .details("Mission 5")
        .start_timestamp(1234)
        .end_timestamp(5678)
        .large_image("cover-art")
        .large_text("Cover Art")
        .small_image("icon")
        .small_text("Icon Text")
        .party("party-id", 2, 4)
        .button("Join", "https://example.com/join")
        .build();

    let value = activity_to_value(&activity);

    assert_eq!(value["state"], "Playing campaign");
    assert_eq!(value["details"], "Mission 5");

    let timestamps = value
        .get("timestamps")
        .and_then(Value::as_object)
        .expect("timestamps serialized");
    assert_eq!(timestamps["start"], 1234);
    assert_eq!(timestamps["end"], 5678);

    let assets = value
        .get("assets")
        .and_then(Value::as_object)
        .expect("assets serialized");
    assert_eq!(assets["large_image"], "cover-art");
    assert_eq!(assets["small_text"], "Icon Text");

    let party = value
        .get("party")
        .and_then(Value::as_object)
        .expect("party serialized");
    let size = party["size"].as_array().expect("party size array");
    assert_eq!(size[0], 2);
    assert_eq!(size[1], 4);

    let buttons = value["buttons"].as_array().expect("buttons serialize");
    assert_eq!(buttons.len(), 1);
    assert_eq!(buttons[0]["label"], "Join");
}

#[test]
fn manual_activity_validation_matches_serialization() {
    let activity = Activity {
        state: Some("Multiplayer".to_string()),
        party: Some(ActivityParty {
            id: Some("group".into()),
            size: Some([1, 4]),
        }),
        assets: Some(ActivityAssets {
            large_image: Some("hero".into()),
            large_text: Some("Hero".into()),
            ..ActivityAssets::default()
        }),
        ..Default::default()
    };

    activity.validate().expect("activity should be valid");

    let value = activity_to_value(&activity);
    assert_eq!(value["state"], "Multiplayer");
    assert!(value.get("assets").is_some());
}

#[test]
fn activity_type_serializes_correctly() {
    let activity = ActivityBuilder::new()
        .activity_type(ActivityType::Listening)
        .build();

    let json = serde_json::to_value(&activity).unwrap();

    assert_eq!(json["type"], 2);
}

#[test]
fn all_activity_types_serialize_correctly() {
    let cases = [
        (ActivityType::Playing, 0),
        (ActivityType::Listening, 2),
        (ActivityType::Watching, 3),
        (ActivityType::Competing, 5),
    ];

    for (activity_type, expected) in cases {
        let activity = ActivityBuilder::new()
            .activity_type(activity_type)
            .build();

        let json = serde_json::to_value(activity).unwrap();

        assert_eq!(json["type"], expected);
    }
}

#[test]
fn default_activity_omits_type() {
    let activity = Activity::default();
    let json = serde_json::to_value(activity).unwrap();

    assert!(json.get("type").is_none());
}

#[test]
fn activity_type_deserializes_from_json() {
    let activity: Activity =
        serde_json::from_str(r#"{"type": 2}"#).unwrap();

    assert_eq!(activity.r#type, Some(ActivityType::Listening));
}

#[test]
fn status_display_type_serializes_and_deserializes_correctly() {
    for (display_type, expected) in [
        (StatusDisplayType::Name, 0),
        (StatusDisplayType::State, 1),
        (StatusDisplayType::Details, 2),
    ] {
        let activity = ActivityBuilder::new()
            .status_display_type(display_type)
            .build();
        let json = serde_json::to_value(activity).unwrap();
        assert_eq!(json["status_display_type"], expected);
    }

    let default_json = serde_json::to_value(Activity::default()).unwrap();
    assert!(default_json.get("status_display_type").is_none());

    let parsed: Activity = serde_json::from_str(r#"{"status_display_type": 2}"#).unwrap();
    assert_eq!(parsed.status_display_type, Some(StatusDisplayType::Details));
}
