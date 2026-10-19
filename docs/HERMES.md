# Talk to your Hermes agent

Sherka can be the voice and desktop avatar for an existing **Hermes Agent** server. VoiceStudio transcribes your speech locally; Hermes handles the conversation with its own personality, memory, model and tools; VoiceStudio reads the reply.

In this mode, Sherka does not send its character prompt or memories, run its own web search, consolidate memories, or choose Hermes's model/token budget. Your server configuration remains in charge. Emotional portraits and mood analysis are excluded from Hermes mode; the avatar stays neutral. Listening and speaking indicators remain available.

## 1. Enable the Hermes conversation API

On the server, generate a random access key:

```sh
python -c "import secrets; print(secrets.token_urlsafe(32))"
```

Keep the generated value private. This is a **Hermes API key**, not your OpenRouter, Z.ai or other model-provider key.

Edit the profile's `.env` file:

- Default profile: `~/.hermes/.env`
- Named profile: `~/.hermes/profiles/PROFILE_NAME/.env`

Add or update these entries, replacing the example values:

```dotenv
API_SERVER_ENABLED=true
API_SERVER_KEY=PASTE_YOUR_GENERATED_KEY
API_SERVER_PORT=8642
API_SERVER_HOST=127.0.0.1
```

If Sherka runs on another computer on your local network, set `API_SERVER_HOST` to the server's **private LAN IP address** instead. Use HTTPS for an Internet-facing endpoint. Sherka accepts HTTP on loopback or private IP addresses and never follows redirects with your key.

Restart the Hermes gateway using its usual service manager. With a standard Hermes CLI installation, use `hermes gateway restart` for the default profile, or `hermes -p PROFILE_NAME gateway restart` for a separately managed profile. Custom system services should be restarted using their existing service setup.

## 2. Several profiles on one server

For a shared listener, merge this setting into the default profile's `~/.hermes/config.yaml`:

```yaml
gateway:
  multiplex_profiles: true
```

Create a distinct `API_SERVER_KEY` in **each named profile's own `.env` file**. Keep the default key in `~/.hermes/.env`. Restart the gateway after configuration changes.

The default profile uses the ordinary address. Named profiles use Hermes's `/p/PROFILE_NAME/` routing internally. **In Sherka, enter the server address without `/p/...` and select the profile separately.** The default profile's key cannot authenticate to another profile.

The shared gateway serves eligible profiles; Hermes profiles configured as standalone or parked are excluded. If a profile runs on its **own port**, enter that listener's address and select `default` in Sherka: the listener already identifies the profile.

## 3. Connect from Sherka

1. Open **Companions → Voice & models** and select the companion to configure.
2. Choose **Hermes · remote agent**.
3. Enter the server address, such as `http://192.168.1.50:8642`.
4. Choose a saved **Hermes profile**, or type its exact name. Use `default` for the main profile.
5. Paste that profile's key and click **Save key**.
6. Click **Test and refresh**, then **Save these choices**.
7. Choose your shared VoiceStudio transcription model in Settings, then choose the speech engine and voice in the companion’s **Avatar & voice** editor.

Sherka remembers successfully connected server/profile pairs. The conversation API does not advertise a global profile directory, so a new profile must first be entered by name. Switching the server or profile switches the key scope; keys are protected by Windows Credential Manager and excluded from exports.

## Conversations, interruptions and approvals

Each Sherka companion/server/profile combination gets a separate, persistent Hermes conversation. It survives restarting Sherka and follows session changes caused by Hermes context compression. Telegram conversations are not automatically merged into this thread.

While using Hermes, reveal the widget controls and click **+ — New Hermes conversation**, or type **`/new`** in the conversation box. This interrupts the current reply and starts a fresh thread on your next message. Previous conversations remain on Hermes, and its long-term memory is preserved. Sherka's conversation view then shows the new thread. The button is absent for ordinary model providers.

Click **Stop Hermes (/stop)** in the conversation or widget, or type **`/stop`**. The command clears local speech and targets only the active remote run. It does not stop the gateway, reboot the server or create a new conversation. For the HTTP API, Sherka maps the command to `POST /v1/runs/{run_id}/stop`, then checks `GET /v1/runs/{run_id}` for a terminal status. Sending the literal text as an ordinary chat message would not guarantee command handling. Failed or unconfirmed stops stay retryable and prevent overlapping remote work. Automatic voice interruptions use the same run-scoped stop. Remote tool interruption is cooperative: a tool may need time to stop, and actions already completed are not undone. A lost stream is reported as an interrupted connection, not a completed answer.

When Hermes requests tool approval, Sherka shows the request in its conversation panel with **Allow once** and **Deny**. It never approves tools automatically. Reasoning and technical tool events are excluded from speech; assistant text is streamed into VoiceStudio's existing speech queue.

## Troubleshooting

- **Cannot connect:** check the server address, API bind address, port and firewall. An existing desktop/WebSocket connection does not necessarily mean the HTTP API is enabled.
- **HTTP 401/403:** use the selected profile's Hermes key. Verify you did not paste a model-provider key.
- **HTTP 404 on a named profile:** check its exact name and shared-gateway routing. For a separately hosted profile, use its own listener and `default`.
- **Missing API features:** update Hermes. Sherka checks `/v1/capabilities` for native session streaming and run interruption.
- **Text works but speech does not:** check VoiceStudio and the companion's voice settings; Hermes is not responsible for local speech synthesis.
- **Unexpected tool actions after speaking:** inspect the transcribed message in the conversation. Hermes receives that text as your instruction. Local search, memories and emotion prompts remain bypassed. Stop the task, then type the corrected instruction if recognition is uncertain; microphone recordings are not retained for retrospective diagnosis.

Protocol references: [Hermes API Server](https://hermes-agent.nousresearch.com/docs/user-guide/features/api-server) and [programmatic integration](https://github.com/NousResearch/hermes-agent/blob/main/website/docs/developer-guide/programmatic-integration.md).
