# Security and privacy

Please report vulnerabilities through GitHub private vulnerability reporting. Do not post secrets, databases, voice recordings or personal conversations in public issues.

Sherka 0.3.3 is the initial supported public version. The panel and widget have distinct native permissions. Network output and imported files are treated as untrusted data. Update downloads are signed and require a matching signed version.

Conversations use local services by default. Selecting a remote provider sends the companion prompt, retained exchanges and recalled memories to that provider, including memory processing requests. VoiceStudio audio stays local. Windows Credential Manager stores keys by provider and exact API base; keys are excluded from databases and exports. Network errors never expose arbitrary provider response bodies or credentials. Optional manual update checks contact GitHub without dialogue or memory data. External applications have their own privacy policies and settings.

Publication checks exclude personal data and signing keys. They reduce accidental disclosure but are not a substitute for reviewing staged files and screenshots.
