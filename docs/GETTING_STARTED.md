# Getting started

## Prepare voice and intelligence

1. Install [VoiceStudio for Windows](https://github.com/debpalash/VoiceStudio/releases). Download a ready TTS engine and speech recognition model, then create or select a voice. OmniVoice and Parakeet TDT v3 were used during development.
2. For offline intelligence, install [LM Studio](https://lmstudio.ai/), download a conversation model suitable for your PC and start its [local server](https://lmstudio.ai/docs/developer/core/server). Start with at least 8,192 context tokens for a model such as Dolphin 8B. Alternatively, use your own OpenRouter, Z.ai, OpenAI, Anthropic or compatible API key; see [provider setup](PROVIDERS.md).

These applications, voices and models are separate downloads with their own licenses and hardware requirements. Response speed depends on your hardware and choices.

## Pick your download

From [official Releases](https://github.com/ArthurReboulSalze/Sherka-Compagnon/releases/latest):

- **Installer:** run `Sherka-Compagnon_<version>_x64-setup.exe`, then launch from the Start menu. Installation is per user and offers WebView2 if needed.
- **No installation:** extract the entire `Sherka-Compagnon_<version>_windows-x64-portable.zip` into a writable folder and run `sherka-compagnon.exe`. Do not run inside the ZIP. WebView2 must already be present; get it from [Microsoft](https://developer.microsoft.com/en-us/microsoft-edge/webview2/).

Windows 11 x64 is the tested platform. Windows 10 with WebView2 may work but has not received equivalent validation. Executables do not carry an Authenticode certificate, so SmartScreen may ask for confirmation. Sherka update signatures are a separate mechanism.

The [Microsoft Visual C++ v14 x64 runtime](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist) is also required. Install its current x64 package if it is missing; the Sherka installer does not bundle it.

## Your first conversation

The companion appears alone, with its microphone off. Hover over it, open the control panel, then **Companions → Voices & models** (**Compagnons → Voix & modèles** in French). Choose your [interface language](LANGUAGES.md) in Settings.

The original Sherka comes with a personality and instructions in all seven languages and follows your interface choice automatically. You do not need to rewrite her character. Custom companions keep their own language.

Sherka starts installed VoiceStudio when its local service is stopped. Typical addresses are `http://127.0.0.1:1234` for LM Studio and `http://127.0.0.1:3900` for VoiceStudio. Choose the conversation provider for this companion, refresh its catalog, choose a model, then choose the VoiceStudio transcription model and save. Open the companion’s **Avatar & voice** editor to choose its TTS engine and voice. For a remote provider, save a protected API key. If the catalog has no context limit, Sherka uses 64,000 tokens by default; adjust the ceiling to your model if needed. Replies default to an 8,192-token reserve within the model's limits. Provider charges and privacy policies apply.

Under **Microphone & listening**, select input and output. Start with headphones, enable the microphone and speak normally. Confirmed speech can interrupt the current response. You can also type, with or without spoken replies.

## Troubleshooting

Installed, no-install and development executables share the same user data unless a development override is configured. Opening a newer copy can migrate that data; it does not update an older executable elsewhere on your PC.

From 0.5.1 onward, incompatible or unreadable data opens a recovery screen with **Check for updates** and a local error detail. No conversation or voice service starts in that mode. Do not reset or delete your profiles to resolve a version mismatch.

If an older release closes immediately with no screen, download the latest official installer and install it over the existing copy. Check the installation folder: Windows may remember a previous custom location. See [startup recovery](STARTUP_RECOVERY.md).

- **VoiceStudio is preparing:** its first launch or model loading can take time. Check its window, then **Retry VoiceStudio**.
- **No models:** check the LM Studio server, downloads and address, then refresh.
- **No voice:** select a ready TTS engine and an existing VoiceStudio voice.
- **Interruptions or delays:** try headphones, check devices and reduce model load. Speaker echo needs further testing across rooms and hardware.

Use **Quit Sherka** to exit completely. Closing the control panel only hides it.

For an existing Hermes agent, choose **Hermes · remote agent** and follow the [Hermes connection guide](HERMES.md). Your server handles personality, memory and tools; VoiceStudio handles your local speech.
