use sherka_core::{
    Companion,
    model::{ImagePlacement, InitiativeConfig, VoiceServiceConfig},
    window::Bounds,
};
#[test]
fn legacy_voice_moves_once_to_global_settings_and_does_not_follow_profile_switches() {
    let root = tempfile::tempdir().unwrap();
    let core = Companion::open(root.path()).unwrap();
    let original = core.snapshot().unwrap();
    let a = original.session.profile_id;
    let b = original.profiles.iter().find(|p| p.id != a).unwrap().id;
    let mut p = core.store.profile(a).unwrap();
    p.details.connections.voice_studio_url = "http://127.0.0.1:4567".into();
    p.details.connections.stt_model = Some("my-transcriber".into());
    p.details.connections.tts_voice = Some("voice-a".into());
    core.store.update(a, p.revision, p.details).unwrap();
    let mut settings = original.settings;
    settings.voice_service = None;
    core.store.save_settings(&settings).unwrap();
    drop(core);
    let mut core = Companion::open(root.path()).unwrap();
    assert_eq!(core.voice_service_config().url, "http://127.0.0.1:4567");
    assert_eq!(
        core.profile(b)
            .unwrap()
            .details
            .connections
            .stt_model
            .as_deref(),
        Some("my-transcriber")
    );
    let exported = core.store.export_profile(a).unwrap();
    assert!(!exported.contains("4567"));
    assert!(!exported.contains("my-transcriber"));
    assert!(exported.contains("voice-a"));
    core.switch_profile(b).unwrap();
    assert_eq!(
        core.profile(b)
            .unwrap()
            .details
            .connections
            .voice_studio_url,
        "http://127.0.0.1:4567"
    );
    core.set_voice_service(VoiceServiceConfig {
        url: "http://127.0.0.1:5678".into(),
        stt_model: Some("new-transcriber".into()),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(
        core.profile(a)
            .unwrap()
            .details
            .connections
            .tts_voice
            .as_deref(),
        Some("voice-a")
    );
    drop(core);
    let core = Companion::open(root.path()).unwrap();
    assert_eq!(
        core.profile(a)
            .unwrap()
            .details
            .connections
            .voice_studio_url,
        "http://127.0.0.1:5678"
    );
}
#[test]
fn geometry_is_independent_durable_and_rejects_late_cell_writes() {
    let root = tempfile::tempdir().unwrap();
    let mut core = Companion::open(root.path()).unwrap();
    let snapshot = core.snapshot().unwrap();
    let a = snapshot.session.profile_id;
    let b = snapshot.profiles.iter().find(|p| p.id != a).unwrap().id;
    let bounds = Bounds {
        x: 40,
        y: 80,
        width: 310,
        height: 370,
    };
    let image = ImagePlacement {
        x: -50.0,
        y: 12.0,
        size: 960.0,
        moved: true,
    };
    core.save_bounds(bounds).unwrap();
    core.save_widget_cell(a, Some(image), Some(125.0)).unwrap();
    core.switch_profile(b).unwrap();
    assert!(core.snapshot().unwrap().settings.widget_bounds.is_none());
    assert!(core.save_widget_cell(a, None, None).is_err());
    core.save_bounds(Bounds { x: 400, ..bounds }).unwrap();
    drop(core);
    let mut core = Companion::open(root.path()).unwrap();
    core.switch_profile(a).unwrap();
    let settings = core.snapshot().unwrap().settings;
    assert_eq!(settings.widget_bounds, Some(bounds));
    assert_eq!(settings.desktop_layouts[&a].image, Some(image));
    assert_eq!(settings.desktop_layouts[&b].bounds.unwrap().x, 400);
    assert!(!InitiativeConfig::default().enabled);
    assert!(
        InitiativeConfig {
            enabled: true,
            min_minutes: 10,
            max_minutes: 5
        }
        .validate()
        .is_err()
    );
}
