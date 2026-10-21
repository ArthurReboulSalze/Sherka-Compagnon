# Roadmap

Sherka 1.3 is a fairly complete companion: voice conversation with interruption, memory, web search, persistent moods, ComfyUI scenes, AI-assisted character creation and companion packs. Companions keep their current presentation, with seven static expressions and half-second crossfades; portrait or mouth animation is not planned.

The next phase focuses on performance, polish and wider provider support. Further features will follow user requests. These directions are not delivery promises.

## Performance

- Measure end-to-end latency (end of speech, first text, first audio) and reduce it where Sherka adds overhead.
- Smooth long replies within TTS provider capabilities.
- Diagnose slow VoiceStudio launches and clarify model preparation status.
- Keep the desktop widget, image cells and ComfyUI coordination light on CPU, GPU and memory while conversing.
- Offer hardware-appropriate presets for LLM, STT and TTS choices.

## Polish features and interface

- Refine existing settings, labels and help where they are hard to discover or understand.
- Improve speaker listening through broader acoustic and echo testing.
- Improve the character creator's identity, framing and expression fidelity, and refine model-selected expressions within the seven image slots. Hermes remains neutral.
- Better memory relevance and corrections with smaller models.
- Accessibility, translation review and guided backup/restore.

## More providers

- Additional local runtimes such as Ollama, alongside LM Studio and compatible APIs.
- Additional cloud APIs, broader model compatibility, cost visibility and the OpenAI Responses API.
- VoiceStudio authentication and broader credential-service coverage beyond Windows Credential Manager and Linux Secret Service.

## Distribution

- A Windows signing certificate to reduce SmartScreen friction.
- More Windows configurations and audio devices tested; ongoing update and migration validation between published versions.
- Physical Linux desktop/audio validation, wider distribution compatibility and a Linux update mechanism beyond manual Debian packages.

Visual changes must respect reduced-motion preferences and keep the desktop widget unobtrusive.
