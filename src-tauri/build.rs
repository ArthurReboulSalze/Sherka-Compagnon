fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "get_snapshot",
            "get_widget_visibility",
            "get_voice_service",
            "retry_voice_service",
            "switch_companion",
            "apply_mutation",
            "get_history",
            "get_memories",
            "edit_memory",
            "search_archive",
            "export_memories",
            "export_profile",
            "widget_action",
            "discover_language_models",
            "discover_voices",
            "get_audio_devices",
            "get_conversation",
            "send_message",
            "get_update_status",
            "check_update",
            "install_update",
        ]),
    ))
    .expect("Impossible de construire les ressources de Sherka");
}
