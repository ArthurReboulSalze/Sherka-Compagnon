# User guide

## Clear conversations and manage saved facts

In **Conversation**, choose **Clear conversation**. The confirmation keeps saved facts by default: facts entered manually, requested explicitly, or approved with **Remember this** survive. Messages, source quotations, automatic suggestions, summaries and searchable archives are removed for that companion. Select **Also delete saved facts** only when you want those removed too. The companion's personality, images, voice and model settings stay intact.

You can also say **“Clear our conversation”** or type **`/clear`**. Sherka asks for confirmation: say **“confirm deletion”** or type **`/confirm-clear`** within three minutes. Use **`/cancel`** to cancel. Another ordinary message, a companion switch or restarting the app cancels the pending request. A short “yes” or “I confirm” also works while this confirmation is pending; it has no deletion meaning otherwise. Confirmation applies only to the current companion and conversation. This preserves saved facts and runs locally without asking the language model to decide what to delete. The spoken acknowledgement does not create a new history entry. Requests must be explicit; quoted instructions and negated examples do not trigger deletion. Supported examples include “Efface notre conversation”, “Borra nuestra conversación”, “Cancella la nostra conversazione”, “Lösche unseren Chatverlauf”, “清空我们的对话” and “会話履歴を削除して”.

In **Memory**, **Delete this fact** removes an individual saved fact, including from future memory exports. Earlier exchanges are excluded from recall so the fact is not immediately reconstructed. Clear the conversation as well if you also want its historical messages removed. Existing exports or backups are not modified.

For **Hermes**, the panel action clears only Sherka's local history and resets its client conversation reference. Hermes owns its server conversations, memory and tools; requests spoken to Hermes continue to go to Hermes. For hosted model providers, local deletion does not remove data already received by the provider.

## On your desktop

When Sherka starts, the widget first shows only the Sherka logo with a loading animation and the state of VoiceStudio; your companion appears as soon as the voice service is ready (immediately if it is already running or switched off). If VoiceStudio fails to start, the companion appears anyway so you can still type, and the reason is shown in the panel.

Hover to reveal controls. Click to keep them visible; click again or press Escape to release them. Drag the portrait to move it and use its corner to resize. The panel and widget can be hidden independently.

The widget menu and selecting a companion card in the board both activate that companion. The notification icon restores the widget and clicks, opens the panel, pauses voice or exits.

**Text** toggles a transparent input cell below the avatar's controls. Type or paste, press Enter to send and Shift+Enter for a new line. Its checkbox chooses whether to read the reply aloud. Drop text or a text file onto the companion to prepare a message; nothing is submitted until you send it. You can also attach up to four PNG, JPEG or WebP images when the selected model supports vision. Unknown model capabilities require explicit confirmation in the advanced connection settings; a model advertised as text-only cannot accept images. Attachments are sent for that turn only, without storing their pixels in conversation history or memory.

**Image** toggles the optional conversation illustration cell; configure it under Companions first. The image generator always starts off: whenever Sherka starts or you switch to a companion, its images are switched off (and ComfyUI is not started) until you press **Image** again. Its pencil button shows the latest scene prompt; edit it to regenerate that image with your own wording. Drag it to change its position relative to the avatar and resize it from its corner. Choose the image count and slideshow duration in the illustration settings. Images start preparing after text generation, while speech continues. Pressing Image again hides the cell and disables subsequent generations. Both cells follow the widget without opening additional windows. Controls follow the visible avatar bounds, and the character's chosen color supplies its active halo. Application themes are selected in Settings.

## Three-way conversations

You can bring a second companion into the conversation. The active companion stays the **master** and keeps the microphone, the text cell, **Stop** and **Hide**; the one you invite is the **guest**.

- **Invite.** Click **Add a companion** (person-plus icon) in the widget toolbar and choose a companion in the picker. The guest appears in its own small transparent window. Drag and resize it like the main portrait (same size limits); its position is remembered per companion, and it hides and shows with the widget.
- **Recent.** For each master, the picker lists **Recent** first: **Resume the last conversation** with the latest guest, then up to two other recent guests. Each companion keeps the three-way thread in its own history, so inviting the same guest again resumes it with context, even after a restart.
- **Remove.** Click the current guest in the picker, use **Remove from the conversation** on the guest's window, or press **End** in the banner shown in the panel's Conversation view. The guest also leaves when you switch the master or restart Sherka; it is never kept between sessions.
- **Who answers.** Start with a name, as in “Minerva, tell me a joke”, and that companion answers. Names are matched within the first three words, ignoring case and accents, and forgive one wrong letter in names of five letters or more to absorb recognition slips. Otherwise one companion, chosen at random, answers. Only one companion answers your message, and it always has something to say. The other one does not answer straight away: it reacts later if you stay silent (see Relay and chat below). If spontaneous conversation is enabled for the master, it also works with a guest: one companion opens and the other reacts right after, then they wait for you.
- **Arrival.** When you add a guest, the master makes one short spoken remark about it in character (for instance teasing an irritable guest), and only the master speaks. If the master has never talked with that companion, it is told who the guest is first. Nothing is recorded as your message. Adding a guest while the companion is busy skips the remark. If you then say nothing, the usual silence mechanism starts from the remark (see Relay and chat below): after the delay the guest reacts to it, then, after a quick silence of one second, the two companions start talking to each other.
- **Voices.** Each companion speaks with its own voice, one after the other, never at the same time. While the first companion speaks, the other one prepares its reaction in the background; nothing is shown or heard until it is its turn, and you can speak at any time. Speaking over them drops the prepared reaction too.
- **Relay and chat.** When you speak, exactly one companion answers. Once it has finished, the companions wait for the delay. If you stay silent, the other companion reacts to your message and to the answer (it can stay silent, and then waits a further delay before being asked again). After a quick silence of one second, the first one reacts to that reaction, and from then on they chain replies to each other with a short pause, one after the other, until you speak or type, press **Stop** or pause, or they have said 20 replies since your last message or both have nothing to add. Any activity of yours (speaking, typing, **Stop**, pause) ends the exchange and drops the reaction that was being prepared; after that, the cycle starts over with a single answer and the full delay. Choose the delay, or turn all of this off (then only one companion answers and nothing follows), in Settings → **Three-way conversations** (off, 5, 8, 10 or 15 seconds; 8 by default). Speaking, typing in the text cell, **Stop**, pausing, removing or changing the guest or switching the master cancels it; muting the microphone or hiding the widget ends it. Relay and chained replies are not messages of yours and are not memorized, but your own message still is: the companions learn about you even in a trio, once they fall silent. The same settings card has an optional **Your first name**: your companions use it to know it is you speaking in a three-way conversation (empty: "the user"). With a paid cloud provider, every one of these replies is billed, including reactions that were prepared and then discarded because you spoke, so you may pay for more requests than you hear.
- **Quick replies.** Turn on **Favor quick replies** in the same settings section (off by default) to make both companions answer in one or two short spoken sentences, so speech starts sooner.
- **Interrupt.** Speaking over them, Stop or pausing stops both companions. The part that was heard is kept in history and shared with the other companion.
- **Shared model.** The guest uses the master's provider and model, so only one LM Studio model stays loaded. It keeps its own personality, language, voice, expressions, memory and mood. Both can learn facts about you; neither remembers the other's words as yours. If a companion who left earlier appears in the history, the remaining one is told that those lines came from someone who has since left and can no longer hear.

Not available in a three-way conversation:

- **Hermes** companions cannot be master or guest; the button is hidden for a Hermes master and Hermes profiles are disabled in the picker.
- A companion without a configured voice cannot be invited.
- Conversation illustrations are disabled for both (the **Image** button is greyed out), and attached images are seen only by the first speaker.
- Clearing the conversation, memory optimization and their confirmations apply to the master only. Memory requests and web searches go to the companion who speaks first, so name the companion you want to remember something.

Small models sometimes write the other companion's lines or stay silent too often; a larger model gives more natural exchanges. Downgrading Sherka after the update is not supported, because the history format changed.

## Characters and voices

Create, rename, duplicate or import a profile under **Companions**. Each keeps its personality, instructions, image, model, voice and history. A duplicate starts without conversations or memories. Deletion requires the character's name; activate another profile before deleting the active one.

The starter collection includes Sherka and four characters with seven images each: **Botsy**, a curious tinkerer; **Alba**, warm and playful; **Kit**, precise and loyal with dry, deadpan humor; and **Minerva**, irritable, impatient and sharp-tongued. Their presets are available in all seven interface languages. The four new characters use the interface language at creation and keep it afterwards; only the original, unmodified Sherka follows later interface language changes. Existing personal profiles stay intact, and deleted starter characters are not recreated on restart. Orion is no longer included as a starter example.

Open **Avatar & voice** on the character’s portrait to preview and assign its seven expressions: neutral, joy, sadness, anger, surprise, reflection and tenderness. Built-in Sherka includes a matching set. Choose its VoiceStudio engine and voice here, or use the shortcut below the portrait. Companions groups the conversation provider, model, context, reasoning, illustrations and Internet access mode. Settings holds the global search provider and API keys, VoiceStudio connection and transcription, local startup, audio devices and updates. Help & about gives concise setup guidance. No personal voice profile is shipped. Switching characters cancels the previous response and manages the corresponding LM Studio model. Reasoning controls follow advertised capabilities.

To regenerate an existing companion's portrait or expressions, use **Regenerate images** below its portrait: the studio opens with its current images, and applying the result changes images only. To generate a new character, use **Create with AI** above the companion list. Review the LLM's proposed sheet, choose the portrait checkpoint and the three Klein models, then follow three fixed stages: approve the frontal neutral portrait on solid gray, generate the six emotions individually (or fill in missing ones), and remove all seven backgrounds. Click images to enlarge them. Review the cutouts before adding the companion and choosing its voice. Name and personality edits do not require new portraits. You can also start from your own picture, which becomes the neutral portrait (a model that sees images also describes it), and use each image's **Prompt** button to view or edit the exact prompt sent to ComfyUI. Drafts stay available within the current app session; saving happens only at the final step. See the [creation guide](COMFYUI.md#character-creation) for model and node requirements.

### How expressions change

For a companion with expression images, the conversation model chooses one expression at the beginning of each reply. Sherka reads this small marker from the existing text stream and removes it before displaying, speaking or storing the answer. No second model request or microphone mood analysis is involved. The choice describes the tone of the reply; accuracy depends on the model following the instruction. It does not diagnose how you feel.

The expression lasts through the reply and speech playback. Images crossfade over half a second, including the return to neutral; the next image loads before the transition starts. Rapid changes keep only the latest pending expression. The system's reduced-motion preference disables this animation. With persistent moods disabled, finishing, stopping, switching companions or restarting returns the avatar to neutral. Short examples help smaller models follow the protocol. Common alternative emotion names are accepted, and a conservative phrase-based fallback uses the visible reply when the model omits its marker. Leading thinking blocks are excluded. Unrecognized tone, or too little spare context for the expression instruction, uses neutral; a missing expression image falls back to the neutral portrait. **Hermes always stays neutral**, with its listening/speaking indicators. Its saved expression images remain available if you later choose another provider.

Choose local PNG/WebP files up to 10 MiB each, with dimensions up to 4096 × 4096. Sherka validates them, preserves transparency and stores normalized copies up to 1024 × 1024 in the character’s database as lossless WebP, which is smaller than PNG and keeps every pixel. You can move the original files afterwards. Resetting an expression removes that slot and uses the neutral portrait; resetting the neutral portrait restores the bundled base image. Images never go to the conversation provider. Built-in images are bundled with the app and can be replaced individually.

Sherka can also generate separate [conversation illustrations through ComfyUI](COMFYUI.md). Avatar expressions use the images selected here, whether supplied by you, bundled or produced with **Create with AI**.

## Persistent moods

In **Companions → Companion moods**, enable **Persistent moods** for that character. It is off by default. Each completed text reply updates a separate local mood reservoir, which survives companion switches and application restarts. Audio playback and its interruptions do not count as extra exchanges. The existing half-second avatar crossfade is preserved.

Repeated emotional exchanges can make an expression last longer. Every completed exchange decays the scores, so neutral exchanges eventually restore the neutral portrait. Kindness and affection can de-escalate anger faster. Scores have individual caps and a shared budget; a switching margin avoids flickering between close emotions. Repeating the same model tag without fresh user cues receives diminishing reinforcement, preventing a permanently angry persona from charging itself forever. Mood scores are never fed back into the model prompt. Detection still depends on the model’s expression tag, with conservative multilingual cues helping smaller models.

**Reset mood** is available in Companions and beside the conversation deletion button. It resets only mood points, leaving messages and saved facts intact. Resetting during a reply prevents that in-flight reply from restoring the previous mood. The conversation deletion dialog can reset mood too; deleting the conversation together with saved facts always includes mood. Disabling persistence restores immediate expressions without deleting the saved mood; use Reset mood for a fresh start. Copies and companion exports carry the preference, but start with a neutral mood and no accumulated points. Hermes does not use local moods.

The widget toolbar, companion picker, text cell and image cell follow the board’s selected color theme. The avatar’s glow keeps the character’s own accent.

## Voice and memory

Microphone activation is explicit; listening can continue during replies. Turning the microphone off only stops listening: a reply already under way is finished and spoken, then the microphone closes. **Stop response** stops the response; **Pause all voice activity** stops input and output. The microphone is off after startup and updates.

With Hermes, use **Stop Hermes (/stop)** in the conversation or the stop button in the widget toolbar, or type **`/stop`**. This interrupts the companion's current remote task and local speech; it never shuts down or restarts the server. Sherka checks the server's task status before confirming the stop. If confirmation fails, retry the button before sending more work. Already completed actions are not undone. Speech recognition can mishear words; use the text box when an exact instruction matters.

Under **Memory & history**, add, correct or forget memories. Essential memories are recalled regularly; topical drawers are retrieved for relevant messages. Ask the companion to remember, correct or forget something: the database write is validated before confirmation. Automatically extracted facts are offered for review.

Use **Optimize memory** in **Settings** or **Memory**, or say **“optimize your memory”** / type **`/optimize-memory`**. The selected model prepares shorter wording and merges overlapping facts, preserving omitted facts. Review the before/after proposal, then choose **Apply optimization** or use **`/apply-memory`**. **`/cancel`** discards the proposal. Nothing is applied automatically. Source links and conversation history remain; new memory edits invalidate an older proposal. The model is instructed to retain names, dates, exceptions and uncertainty, but you should check its wording before applying. Hermes continues to manage its own memory on the server.

Context contains selected recent exchanges and memories, not the entire archive. **Conversation → Context & memory** distinguishes estimates from server usage.

## Measure response times

After each reply, Sherka adds one `turn_timing` line to `audio-events.log` in its data folder. It contains only durations in milliseconds, counted from the end of your sentence (or from sending a typed message): transcript accepted, request ready, first word, first passage sent to the voice, first sound and complete text. No conversation text is recorded. On Windows, list them in PowerShell:

```powershell
Get-Content "$env:LOCALAPPDATA\fr.sherka.compagnon\audio-events.log" | ConvertFrom-Json | Where-Object event -eq turn_timing | ForEach-Object { $_.timing } | Format-Table
```

A long gap before *transcript accepted* usually comes from a speech-recognition engine that transcribes only after you stop, such as Whisper; streaming engines finish almost immediately. A long gap between *request ready* and *first word* with a short conversation often means the model is thinking silently: switch thinking off for that companion. Automatic web search adds a short decision request, plus the search itself when it runs.

## Storage and backups

On Windows, both distributions use `%LOCALAPPDATA%\fr.sherka.compagnon`; the Debian package uses its per-user Linux data folder (see [Linux](LINUX.md)). Each contains a settings database and one database per companion, including images, text and memories. “No installation” describes launching the program; data is not carried in its ZIP folder.

Transient audio stays in memory; Sherka does not save microphone recordings. Interrupted turns distinguish generated text from completed spoken passages. External applications have their own privacy settings.

For a full backup, exit normally and copy the data folder. **Export** now creates a [`.sherka.zip` companion pack](COMPANION_PACKS.md): personality, instructions, connection references, illustration/search settings and all seven expression images, including bundled artwork. History, memories and API keys are excluded. **Import** accepts these archives and legacy JSON profiles (versions 1–5), creating a new independent companion. Review service addresses and authored text before sharing. Duplicating also keeps the character settings and images, with an empty history and memory. Memory exports are separate and may contain personal information.

## Updating

On Windows, open **Settings → Updates**, click **Check for updates**, then **Update and restart**. Debian packages are updated manually from the release page. No conversations, voice profiles or memories accompany the GitHub request.

Downloads are verified against the embedded public key. Installed copies use the installer. Directly launched copies replace their executable in the same folder with a backup during replacement. Voice and the managed model stop before exit. Data stays in your user folder. Back up before major upgrades; avoid downgrading after a database migration.

Explicit requests to forget or correct a saved fact can target both approved facts and automatically extracted memory drafts. Sherka identifies the exact memory and returns a receipt; ambiguous targets are left unchanged. Deleting the discussion is a separate action with confirmation and preserves saved facts by default.
