use sherka_core::{
    catalogs::LanguageModel,
    conversation::{budget, prepare_context_with_avatar},
    illustration::{
        Config, ImagePrompt, ImageReceipt, ImageUnavailable, ModelFamily, checkpoint_short_name,
        generator_ready,
    },
    model::ProfileDraft,
};

fn model(limit: u64) -> LanguageModel {
    LanguageModel {
        vision: None,
        api: None,
        id: "loaded-alias".into(),
        key: "model-key".into(),
        label: "Test".into(),
        loaded: Some(true),
        quantization: None,
        max_context_length: Some(limit),
        loaded_context_length: Some(limit),
        reasoning: None,
    }
}

fn system(
    limit: u64,
    images: Option<&ImagePrompt>,
) -> sherka_core::Result<sherka_core::conversation::PreparedChat> {
    let profile = ProfileDraft {
        name: "Minerva".into(),
        ..Default::default()
    };
    prepare_context_with_avatar(
        &profile,
        &[],
        "Bonjour",
        &Default::default(),
        budget(&profile, &model(limit))?,
        None,
        None,
        false,
        None,
        "",
        sherka_core::trio::Last::User,
        images,
    )
}

fn config(family: ModelFamily, checkpoint: Option<&str>) -> Config {
    Config {
        model_family: family,
        checkpoint: checkpoint.map(Into::into),
        ..Default::default()
    }
}

#[test]
fn the_generator_is_ready_with_a_checkpoint_or_a_workflow_only() {
    assert!(!generator_ready(&Config::default()));
    assert!(generator_ready(&config(
        ModelFamily::Illustrious,
        Some("a.safetensors")
    )));
    let custom = Config {
        workflow: Some(serde_json::json!({})),
        ..Default::default()
    };
    assert!(generator_ready(&custom));
    assert!(ImagePrompt::new(&Config::default(), true, None).is_none());
}

#[test]
fn a_checkpoint_is_named_without_folders_or_extension() {
    assert_eq!(
        checkpoint_short_name("Illustrious/waiNSFW_v14.safetensors"),
        "waiNSFW_v14"
    );
    assert_eq!(
        checkpoint_short_name("sdxl\\base\\Juggernaut.v9.safetensors"),
        "Juggernaut.v9"
    );
    assert_eq!(checkpoint_short_name("plain"), "plain");
}

#[test]
fn without_image_info_the_prompt_is_byte_identical() {
    let plain = system(8192, None).unwrap();
    assert!(!plain.messages[0].content.contains("Image generator"));
    assert!(!plain.messages[0].content.contains("Image receipt"));
    let same = system(8192, None).unwrap();
    assert_eq!(
        serde_json::to_string(&plain.messages).unwrap(),
        serde_json::to_string(&same.messages).unwrap()
    );
}

#[test]
fn the_state_line_names_the_family_and_the_checkpoint() {
    let on = ImagePrompt::new(
        &config(
            ModelFamily::Illustrious,
            Some("models/waiNSFW_v14.safetensors"),
        ),
        true,
        None,
    )
    .unwrap();
    let text = system(8192, Some(&on)).unwrap().messages[0].content.clone();
    assert!(text.contains("Image generator: on (Illustrious: anime-style illustrations built from tags, model waiNSFW_v14)"));
    assert!(text.contains("automatically creates one illustration of this exchange"));
    assert!(text.contains("you cannot see the image"));
    assert!(text.contains("never describe or comment on the finished picture"));
    assert!(!text.contains("Image receipt"));
    let sdxl = ImagePrompt::new(
        &config(ModelFamily::Sdxl, Some("Juggernaut.safetensors")),
        true,
        None,
    )
    .unwrap();
    let text = sdxl.text();
    assert!(text.contains(
        "(SDXL: illustrations described in natural English visual prose, model Juggernaut)"
    ));
    assert!(!text.contains("Illustrious"));
}

#[test]
fn the_off_state_tells_the_user_how_to_switch_it_on() {
    let off = ImagePrompt::new(
        &config(ModelFamily::Sdxl, Some("a.safetensors")),
        false,
        None,
    )
    .unwrap();
    let text = off.text();
    assert!(text.contains("Image generator: off (SDXL, model a)"));
    assert!(text.contains("you cannot switch it on yourself"));
    assert!(text.contains("switch on the image generator"));
    assert!(text.contains("Image button"));
    assert!(!text.contains("automatically creates"));
}

#[test]
fn each_receipt_has_its_own_short_line() {
    let lines: Vec<String> = [
        ImageReceipt::SwitchedOn,
        ImageReceipt::SwitchedOnWithRequest,
        ImageReceipt::SwitchedOff,
        ImageReceipt::Preparing,
        ImageReceipt::NeedsSwitchOn,
        ImageReceipt::Unavailable(ImageUnavailable::NotConfigured),
        ImageReceipt::Unavailable(ImageUnavailable::ThreeWay),
        ImageReceipt::Unavailable(ImageUnavailable::SwitchFailed),
    ]
    .iter()
    .map(ImageReceipt::text)
    .collect();
    let mut unique = lines.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), lines.len());
    assert!(lines.iter().all(|l| l.len() < 260));
    assert!(
        lines[0].starts_with(
            "The user has just switched the image generator on; images will accompany"
        )
    );
    assert!(lines[1].contains("being made now"));
    assert!(lines[2].contains("switched the image generator off"));
    assert!(lines[3].contains("say briefly you are making it"));
    assert!(lines[4].contains("ask them to switch it on; their request is kept"));
    assert!(lines[5].contains("no image model is set up"));
    assert!(lines[6].contains("three-way"));
    assert!(lines[7].contains("nothing changed"));
}

#[test]
fn the_receipt_follows_the_state_in_the_system_text() {
    let prompt = ImagePrompt::new(
        &config(ModelFamily::Illustrious, Some("a.safetensors")),
        true,
        Some(ImageReceipt::Preparing),
    )
    .unwrap();
    let chat = system(8192, Some(&prompt)).unwrap();
    let text = &chat.messages[0].content;
    let state = text.find("Image generator: on").unwrap();
    let receipt = text
        .find("Image receipt: An illustration of the user's request")
        .unwrap();
    assert!(state < receipt);
    assert_eq!(chat.messages.last().unwrap().content, "Bonjour");
}

#[test]
fn an_unavailable_receipt_stands_alone_but_a_failed_switch_keeps_the_state() {
    let three_way = ImagePrompt::unavailable(ImageUnavailable::ThreeWay);
    assert!(!three_way.text().contains("Image generator:"));
    assert!(
        three_way
            .text()
            .contains("Image receipt: The user asked for an image")
    );
    let unset = ImagePrompt::unavailable(ImageUnavailable::NotConfigured);
    assert!(!unset.text().contains("Image generator:"));
    let failed = ImagePrompt {
        receipt: Some(ImageReceipt::Unavailable(ImageUnavailable::SwitchFailed)),
        ..ImagePrompt::new(
            &config(ModelFamily::Illustrious, Some("a.safetensors")),
            false,
            None,
        )
        .unwrap()
    };
    assert!(failed.text().contains("Image generator: off"));
    assert!(failed.text().contains("nothing changed"));
}

#[test]
fn image_text_never_makes_a_small_context_fail() {
    let prompt = ImagePrompt::new(
        &config(ModelFamily::Illustrious, Some("a.safetensors")),
        true,
        Some(ImageReceipt::NeedsSwitchOn),
    )
    .unwrap();
    let mut with_state = 0;
    let mut receipt_only = 0;
    for limit in (1024..=8192).step_by(32) {
        let plain = system(limit, None);
        let images = system(limit, Some(&prompt));
        assert_eq!(plain.is_ok(), images.is_ok(), "context {limit}");
        let Ok(images) = images else { continue };
        let text = &images.messages[0].content;
        if text.contains("Image generator:") {
            with_state += 1;
        } else if text.contains("Image receipt:") {
            receipt_only += 1;
        }
        assert!(images.budget.input_estimate <= images.budget.limit as usize);
    }
    assert!(with_state > 0, "the full text fits in larger contexts");
    assert!(receipt_only > 0, "the receipt alone fits a tight one");
}
