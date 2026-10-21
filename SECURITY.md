# Security and privacy

Please report vulnerabilities through GitHub private vulnerability reporting. Do not post secrets, databases, voice recordings or personal conversations in public issues.

Security fixes target the latest published release. The panel, the transparent widget and the full-screen image viewer have distinct native permissions; the widget and viewer cannot read keys or arbitrary files. Network output, imported packs and files are treated as untrusted data. Windows update downloads are signed and require a matching signed version; Debian packages are updated manually.

Conversations use local services by default. Selecting a remote provider sends the companion prompt, retained exchanges and recalled memories to that provider, including memory processing requests. Hermes mode sends only your messages to your Hermes server, which keeps its own memory and tools. VoiceStudio audio stays on the configured VoiceStudio server. Keys are stored by provider and exact API base in Windows Credential Manager or the Linux Secret Service; they are excluded from databases and exports. Network errors never expose arbitrary provider response bodies or credentials.

Optional features contact other services only when enabled. Web search sends a short query to the chosen engine. ComfyUI receives scene and character-creation prompts and, with IP-Adapter enabled, the companion's neutral portrait. Automatic service startup only launches local installations on loopback addresses and closes only processes Sherka started. Manual update checks contact GitHub without dialogue or memory data. External applications have their own privacy policies and settings.

Publication checks exclude personal data and signing keys. They reduce accidental disclosure but are not a substitute for reviewing staged files and screenshots.
