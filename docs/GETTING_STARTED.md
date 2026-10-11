# Getting started

## Prepare voice and intelligence

1. Install [VoiceStudio for Windows](https://github.com/debpalash/VoiceStudio/releases). Download a ready TTS engine and speech recognition model, then create or select a voice. OmniVoice and Parakeet TDT v3 were used during development.
2. Install [LM Studio](https://lmstudio.ai/), download a conversation model suitable for your PC and start its [local server](https://lmstudio.ai/docs/developer/core/server). Start with at least 8,192 context tokens for a model such as Dolphin 8B. Larger contexts require more memory.

These applications, voices and models are separate downloads with their own licenses and hardware requirements. Response speed depends on your hardware and choices.

## Pick your download

From [official Releases](https://github.com/ArthurReboulSalze/Sherka-Compagnon/releases/latest):

- **Installer:** run `Sherka-Compagnon_<version>_x64-setup.exe`, then launch from the Start menu. Installation is per user and offers WebView2 if needed.
- **No installation:** extract the entire `Sherka-Compagnon_<version>_windows-x64-portable.zip` into a writable folder and run `sherka-compagnon.exe`. Do not run inside the ZIP. WebView2 must already be present; get it from [Microsoft](https://developer.microsoft.com/en-us/microsoft-edge/webview2/).

Windows 11 x64 is the tested platform. Windows 10 with WebView2 may work but has not received equivalent validation. Executables do not carry an Authenticode certificate, so SmartScreen may ask for confirmation. Sherka update signatures are a separate mechanism.

## Your first conversation

The companion appears alone, with its microphone off. Hover over it, open the control panel, then **Réglages → Voix & modèles**. The current UI is in French; this means Settings → Voice & models.

Sherka starts installed VoiceStudio when its local service is stopped. Typical addresses are `http://127.0.0.1:1234` for LM Studio and `http://127.0.0.1:3900` for VoiceStudio. Refresh the catalogs, choose the conversation model, TTS engine, voice and transcription model, then save. Choose items from your own installation.

Under **Microphone & écoute**, select input and output. Start with headphones, enable the microphone and speak normally. Confirmed speech can interrupt the current response. You can also type, with or without spoken replies.

## Troubleshooting

- **VoiceStudio is preparing:** its first launch or model loading can take time. Check its window, then **Réessayer VoiceStudio**.
- **No models:** check the LM Studio server, downloads and address, then refresh.
- **No voice:** select a ready TTS engine and an existing VoiceStudio voice.
- **Interruptions or delays:** try headphones, check devices and reduce model load. Speaker echo needs further testing across rooms and hardware.

Use **Quitter Sherka** to exit completely. Closing the control panel only hides it.
