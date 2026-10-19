# Share a companion

In **Companions**, use **Duplicate this companion** to copy the personality, instructions, images, voice/model choices and illustration settings into a fresh space. The copy has no messages or saved memories. Edit its name and settings, then activate it when ready.

**Export** saves a `.sherka.zip` pack. **Import** accepts this pack or a legacy `.sherka.json` profile (versions 1–5). Import always creates a new identity and does not activate it. Existing companions are untouched.

A pack includes the character's name, personality, instructions, accent, seven expression images, model/voice references, web-search preferences and ComfyUI settings, including positive/negative prompts and a validated custom workflow when present. It does **not** include conversation history, memories, API keys, Hermes sessions, model weights or voice audio files.

Connection addresses and your authored personality/prompts are included: review them before sharing. The recipient must install the referenced models/voices, configure their own connections and supply any required API keys. The original built-in Sherka follows the interface language; a copy or imported character keeps its exported personality language.

## Sherka Companion Archive v1

This is an openly documented Sherka format, not a claim of compatibility with other character-card formats. It uses a standard ZIP container with these eight files:

```text
companion.json
images/neutral.png
images/joy.png
images/sadness.png
images/anger.png
images/surprise.png
images/reflection.png
images/tenderness.png
```

The UTF-8 manifest has four fields:

```json
{
  "format": "sherka-companion",
  "version": 1,
  "profile": {
    "name": "My companion",
    "personality": "Curious, thoughtful, with dry humor.",
    "instructions": "Speak naturally in English.",
    "accent": "amber"
  },
  "images": {
    "neutral": "images/neutral.png",
    "joy": "images/joy.png",
    "sadness": "images/sadness.png",
    "anger": "images/anger.png",
    "surprise": "images/surprise.png",
    "reflection": "images/reflection.png",
    "tenderness": "images/tenderness.png"
  }
}
```

Optional profile fields use the versioned application's [ProfileDraft definition](../crates/sherka-core/src/model.rs), [connection settings](../crates/sherka-core/src/connections.rs), [illustration settings](../crates/sherka-core/src/illustration.rs) and [search settings](../crates/sherka-core/src/search_provider.rs). Omitted settings receive defaults. The [manifest JSON Schema](../config/schemas/companion-v1.schema.json) describes the envelope and profile field names; Sherka also validates the nested settings and workflows at import.

Exports embed the visible image for every slot, including bundled artwork. An unset expression becomes a copy of the neutral image. Imported images do not depend on the receiving application's built-in presets. Hermes can retain these images for later use, but its conversation mode still displays only neutral.

## Validation and compatibility

- Stored and Deflate ZIP compression are accepted. Paths are fixed, case-sensitive and relative. No extra files, directory entries, encrypted entries or symlinks are accepted.
- The archive and its total decoded content are limited to 48 MiB. The manifest is limited to 512 KiB; each PNG to 10 MiB and 4096 × 4096 decoded pixels. Images are normalized to PNG within 1024 × 1024 while retaining transparency.
- All entries are read and validated in memory. Nothing is extracted to a path supplied by the archive. Unsupported versions, unknown manifest/profile fields, invalid images and unsupported workflows fail before a new profile is created.
- Credentials never belong in this format. Importing a pack does not execute its prompts or workflows. Choosing to activate and use the imported companion is a separate action.
- Archive version 1 is independent of the legacy JSON export version (now 5) and database schema (8). Newer required archive behavior must use a new archive version; unsupported versions are rejected explicitly.

See also [the user guide](USER_GUIDE.md) and [ComfyUI setup](COMFYUI.md).
