# Sherka Companion

**A voice. A personality. A memory. A companion that lives on your desktop.**

Sherka turns your local AI models into someone you can talk to. A transparent desktop companion, a voice for each character, and memories that grow with your conversations — on your own PC.

<img src="docs/images/sherka.png" alt="Sherka's control panel on a clean demo profile with no personal conversations" width="760">

[Download for Windows](https://github.com/ArthurReboulSalze/Sherka-Compagnon/releases/latest) · [Get started](docs/GETTING_STARTED.md) · [Documentation](docs/README.md) · [Roadmap](docs/ROADMAP.md)

## Talk, interrupt, pick up again

- **Continuous voice conversations.** Text streams as it is generated. Upcoming speech is prepared while the current passage plays, reducing pauses. Speak again to interrupt a response.
- **A quiet presence.** Your companion appears at startup. Controls show on hover or click; open the control panel whenever you need it.
- **Independent characters.** Each has its own personality, image, voice, model and memories. Switching companions also manages model loading and unloading.
- **Memory you control.** Essential memories and topical drawers, editable in the interface or through conversation.
- **Context awareness.** Sherka budgets against the model's loaded context and reserves room for its reply. Reasoning controls appear when supported.
- **Simple updates.** Check GitHub from Settings and install a signed update, keeping your profiles and memories.
- **Your language.** Seven interface languages and a ready-to-use Sherka in each one. Custom companions keep their own language. [Language settings](docs/LANGUAGES.md).

## Local voice, local intelligence

**[VoiceStudio](https://github.com/debpalash/VoiceStudio) is required** for speech recognition, synthesis and voices. Sherka starts its installed Windows application when needed.

**[LM Studio](https://lmstudio.ai/) is currently the only supported conversation provider.** Start its local server and select your model in Sherka. More providers are planned.

Install both applications and their models separately. Once downloaded, conversation can work offline. Speed depends on your hardware and model choices; headphones are recommended for voice interruption. The microphone is off at startup. Optional GitHub update checks do not send conversations or memories.

## Install it, or just run it

Choose the **Windows x64 installer** or the **no-install ZIP** from [Releases](https://github.com/ArthurReboulSalze/Sherka-Compagnon/releases/latest). Both require WebView2 and the Visual C++ x64 runtime, and store data in your Windows user folder. Choose French, English, Spanish, Italian, German, Simplified Chinese or Japanese in Settings.

Sherka is a working early project. Next come richer avatar customization, speaking and listening animations, expressions and moods. See the [roadmap and current limitations](docs/ROADMAP.md).

## Open source

Sherka's code is licensed under **[GPLv3](LICENSE)**. Dependencies retain their [own licenses](THIRD_PARTY_NOTICES.md). The retrofuturistic artwork is an original, freely inspired creation; no Ulysses 31 artwork is distributed and the project is unaffiliated with the series.

Want to contribute? [Build and test](docs/DEVELOPMENT.md), then open an [issue](https://github.com/ArthurReboulSalze/Sherka-Compagnon/issues) or a pull request.
