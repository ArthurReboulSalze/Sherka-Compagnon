# User guide

## On your desktop

Hover to reveal controls. Click to keep them visible; click again or press Escape to release them. Drag the portrait to move it and use its corner to resize. The panel and widget can be hidden independently.

The widget menu switches companions. The notification icon restores the widget and clicks, opens the panel, pauses voice or exits.

## Characters and voices

Create, rename, duplicate or import a profile under **Compagnons**. Each keeps its personality, instructions, image, model, voice and history. A duplicate starts without conversations or memories. Deletion requires the character's name; activate another profile before deleting the active one.

Transparent PNG/WebP images are validated and re-encoded. Choose a voice under **Choisir sa voix** or the character's settings. No personal voice profile is shipped. Switching characters cancels the previous response and manages the corresponding LM Studio model. Reasoning controls follow advertised capabilities.

## Voice and memory

Microphone activation is explicit; listening can continue during replies. **Arrêter la réponse** stops the response; **Pause vocale globale** stops input and output. The microphone is off after startup and updates.

Under **Mémoire & histoire**, add, correct or forget memories. Essential memories are recalled regularly; topical drawers are retrieved for relevant messages. Ask the companion to remember, correct or forget something: the database write is validated before confirmation. Automatically extracted facts are offered for review.

Context contains selected recent exchanges and memories, not the entire archive. **Conversation → Contexte & mémoire** distinguishes estimates from server usage.

## Storage and backups

Both distributions use `%LOCALAPPDATA%\fr.sherka.compagnon`: a settings database and one database per companion, including images, text and memories. “No installation” describes launching the program; data is not carried in its ZIP folder.

Transient audio stays in memory; Sherka does not save microphone recordings. Interrupted turns distinguish generated text from completed spoken passages. External applications have their own privacy settings.

For a full backup, exit normally and copy the data folder. Profile JSON exports contain appearance, personality and connection references, **without history, memories or keys**. Memory exports are separate and may contain personal information.

## Updating

Open **Réglages → Mises à jour**, click **Vérifier les mises à jour**, then **Mettre à jour et redémarrer**. No conversations, voice profiles or memories accompany the GitHub request.

Downloads are verified against the embedded public key. Installed copies use the installer. Directly launched copies replace their executable in the same folder with a backup during replacement. Voice and the managed model stop before exit. Data stays in your user folder. Back up before major upgrades; avoid downgrading after a database migration.
