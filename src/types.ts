export type Emotion = 'neutral' | 'joy' | 'sadness' | 'anger' | 'surprise' | 'reflection' | 'tenderness';
export interface ComfyStartupConfig {directory:string;python:string;disabled:boolean}
export interface VoiceServiceConfig {url:string;stt_model:string|null;autostart:boolean;directory:string;python:string}
export interface ImagePlacement {x:number;y:number;size:number;moved:boolean}
export interface DesktopLayout {bounds:Snapshot["settings"]["widget_bounds"];image:ImagePlacement|null;text_height:number|null}
export interface InitiativeConfig {enabled:boolean;min_minutes:number;max_minutes:number}
export interface ComfyServiceStatus {attempt:number;url:string;phase:'disabled'|'connecting'|'starting'|'ready'|'error';message:string}
export type Theme = 'gold' | 'sage' | 'ocean' | 'iris' | 'rose' | 'mono';
export interface ImageResolution {width:number;height:number;latent_scale:number;upscale_model:string|null}
export interface ImageReference {enabled:boolean;model:string|null;clip_vision:string|null;weight:number}
export interface IllustrationConfig { resolution?:ImageResolution|null; reference?:ImageReference; enabled:boolean; server_url:string; checkpoint:string|null; model_family:'illustrious'|'sdxl'; checkpoint_folder:string; lora_folder:string; negative_prompt:string; positive_prompt_suffix:string; physical_description:string; appearance_negative_prompt:string; loras:{name:string;strength:number}[]; turbo:boolean; live:boolean; advanced:{enabled:boolean;size:number;steps_first:number;steps_second:number;cfg_first:number;cfg_second:number;sampler_name:string}; image_count:number; slide_seconds:number; workflow:Record<string,unknown>|null }
export interface IllustrationCatalog { ipadapters?:string[];clip_visions?:string[];upscalers?:string[];reference_available?:boolean;transparency_available?:boolean; checkpoints:string[]; loras:string[]; turbo_available:boolean; missing_nodes:string[]; samplers:string[] }
export interface IllustrationStatus { profile_id:string|null; phase:'disabled'|'idle'|'planning'|'generating'|'ready'|'paused'|'error'; image:string|null; images:string[]; message:string|null }
export interface CharacterDraft {
  name:string;personality:string;instructions:string;accent:ProfileDraft['accent'];language:import('./i18n').Language;
  physical_description:string;appearance_negative_prompt:string;portrait_prompt:string;
  emotion_prompts:Record<Exclude<Emotion,'neutral'>,string>;
}
export interface CharacterStudioConfig {
  server_url:string;checkpoint:string|null;model_family:'sdxl'|'illustrious';mood_model:string|null;text_encoder:string|null;vae:string|null;
}
export type CharacterStage='neutral'|'mood'|'transparency';
export interface CharacterStudioCatalog {checkpoints:string[];mood_models:string[];text_encoders:string[];vaes:string[];missing_nodes:Record<CharacterStage,string[]>}
export interface CharacterStudioStatus {
  sequence:number;revision:number;busy:boolean;phase:string;source_profile:string|null;draft:CharacterDraft|null;config:CharacterStudioConfig|null;
  images:Partial<Record<Emotion,string>>;transparent_images:Partial<Record<Emotion,string>>;neutral_validated:boolean;error:string|null;created_profile:string|null;
}
export interface Attachment {name:string;data:string}
export type SearchProvider='duck_duck_go'|'brave'|'tavily'|'searxng';
export interface SearchConfig {provider:SearchProvider;searxng_url:string;max_results:number}
export interface WidgetSurface {left:number;top:number;width:number;height:number;base_width:number;base_height:number}

export interface ProfileDraft {
  illustration?: IllustrationConfig;
  search?: SearchConfig;
  name: string;
  avatar_preset?: 'sherka' | 'botsy' | 'candis' | 'kit' | 'minerva' | null;
  web_search?: SearchMode | null;
  personality: string;
  instructions: string;
  accent: 'amber' | 'teal' | 'violet' | 'coral';
  mood_persistence?: boolean;
  initiative?: InitiativeConfig;
  connections: Connections;
}

export interface Profile extends ProfileDraft {
  id: string;
  revision: number;
  created_at: number;
  updated_at: number;
  avatar: string | null;
  emotion_avatars: Partial<Record<Emotion, string>>;
  message_count: number;
  memory_count: number;
}

export interface Snapshot {
  companion_capabilities: { local_personality: boolean; local_memory: boolean; local_emotions: boolean };
  profiles: Profile[];
  settings: {
    theme: Theme;
    comfyui_startup: ComfyStartupConfig;
    voice_service: VoiceServiceConfig;
    search: SearchConfig;
    desktop_layouts: Record<string,DesktopLayout>;
    web_search: SearchMode;
    ui_language: import('./i18n').Language;
    default_companion: string | null;
    active_profile: string;
    always_on_top: boolean;
    widget_scale: number;
    widget_bounds: { x: number; y: number; width: number; height: number } | null;
    audio: AudioSettings;
  };
  session: {
    profile_id: string;
    session_id: string;
    microphone: 'off' | 'listening' | 'unavailable';
    transcription: 'idle' | 'capturing' | 'transcribing';
    response: 'idle' | 'generating' | 'speaking';
    voice_paused: boolean;
  };
  data_directory: string;
  audio_available: boolean;
  conversation_available: boolean;
  live: LiveConversation;
}

export interface Message {
  web: WebReport | null;
  id: string;
  role: 'user' | 'assistant';
  content: string;
  delivery: 'text_only' | 'completed' | 'interrupted' | 'uncertain';
  heard_content: string;
  created_at: number;
}

export type Mutation =
  | { kind: 'search_config'; config: SearchConfig }
  | { kind: 'voice_service'; config: VoiceServiceConfig }
  | { kind: 'initiative'; id:string;revision:number;config:InitiativeConfig }
  | { kind: 'theme'; theme: Theme }
  | { kind: 'comfyui_startup'; config: ComfyStartupConfig }
  | { kind: 'mood_persistence'; id:string; revision:number; enabled:boolean }
  | { kind: 'reset_mood'; id:string }
  | { kind: 'illustration'; id:string; revision:number; config:IllustrationConfig }
  | { kind: 'web_search'; mode: SearchMode; id?: string; revision?: number;config?:SearchConfig }
  | { kind: 'interface_language'; language: import('./i18n').Language }
  | { kind: 'create_profile'; draft: ProfileDraft }
  | { kind: 'update_profile'; id: string; revision: number; draft: ProfileDraft }
  | { kind: 'switch_profile' | 'duplicate_profile' | 'delete_profile'; id: string }
  | { kind: 'set_avatar'; id: string; revision: number; emotion?: Emotion; base64: string | null }
  | { kind: 'import_profile'; json: string }
  | { kind: 'import_companion_archive'; base64: string }
  | { kind: 'preferences'; preferences: { always_on_top: boolean; widget_scale: number } }
  | { kind: 'connections'; id: string; revision: number; connections: Connections }
  | { kind: 'audio_settings'; audio: AudioSettings }
  | { kind: 'pause_voice' | 'stop_response' };

export type WidgetAction = 'stop_hermes' | 'show_panel' | 'show_widget' | 'hide_widget' | 'toggle_widget' | 'click_through' | 'restore_widget' | 'stop_response' | 'pause_voice' | 'toggle_microphone' | 'quit' | 'quit_without_unloading';

export interface AudioSettings { input_device: string | null; output_device: string | null; speakers: boolean; silence_ms: number; onset_ms: number; interruption_ms: number }
export interface ContextBudget { model: string; max_context: number | null; loaded_context: number; limit: number; input_estimate: number; response_tokens: number; safety_tokens: number; history_kept: number; history_omitted: number; memories_used: number[]; memories_omitted: number }
export type SearchMode = 'off' | 'on_request' | 'auto';
export interface WebReport { query: string; searching: boolean; error: string | null; sources: {title:string;url:string;snippet:string}[] }
export interface LiveConversation { mood: Emotion; emotion: Emotion; hermes_approval?: {run_id:string;request_id:string|null;description:string;allow_once:boolean}|null; web: WebReport | null; tag: { profile_id: string; session_id: string; turn_id: string; generation_id: string } | null; transcript: string; reply: string; heard: string; error: string | null; notice: string | null; budget: ContextBudget | null; memory_receipt: string | null; usage: {length_limited:boolean;prompt_tokens:number|null;completion_tokens:number|null;reasoning_tokens:number|null}|null }
export interface LiveState { session: Snapshot['session']; live: LiveConversation }
export interface AudioDevices { inputs: { id: string; name: string; is_default: boolean }[]; outputs: { id: string; name: string; is_default: boolean }[] }

export interface Connections {
  hermes_profile?: string | null;
  llm_provider?: LlmProvider;
  llm_api_url?: string | null;
  lm_studio_url: string;
  voice_studio_url: string;
  llm_model: string | null;
  llm_model_key: string | null;
  tts_engine: string | null;
  tts_voice: string | null;
  stt_model: string | null;
  language: string;
  speed: number;
  volume: number;
  context_limit: number | null;
  response_tokens: number;
  reasoning: string | null;
  vision_confirmed?:boolean;
}
export type LlmProvider = 'lm_studio' | 'open_router' | 'zai' | 'zai_coding' | 'open_ai' | 'anthropic' | 'custom_open_ai' | 'custom_anthropic' | 'hermes';
export interface LanguageCatalog {
  source: string;
  models: { vision?:boolean|null; id: string; key: string; label: string; loaded: boolean | null; quantization: string | null; max_context_length: number | null; loaded_context_length: number | null; reasoning: {allowed_options: string[];default: string | null} | null }[];
  excluded_models: number;
}
export interface MemoryDraft { content: string; category: string; kind: 'essential' | 'detail' }
export interface MemorySource { id: string; content: string; created_at: number }
export interface MemoryEntry extends MemoryDraft { id: number; revision: number; status: string; approved: boolean; origin: string; created_at: number; updated_at: number; sources: MemorySource[] }
export interface MemoryState { epoch: number; entries: MemoryEntry[]; summary: string; summary_at: number; archive_count: number; excluded_count: number }
export interface MemoryOptimization {guard:{profile_id:string;profile_revision:number;epoch:number};session_id:string;changes:{ids:number[];before:string[];after:string}[];summary:{before:string[];after:string}|null;before_chars:number;after_chars:number}
export type MemoryEdit = { action: 'write'; id: number | null; draft: MemoryDraft } | { action: 'forget' | 'delete' | 'approve'; id: number };
export interface VoiceCatalog {
  service_version: string;
  streaming_transcription: boolean;
  batch_transcription: boolean;
  tts_engines: { id: string; label: string; active: boolean; supports_cloning: boolean; readiness: 'ready' | 'missing_engine' | 'missing_model' | 'unknown'; models: string[] }[];
  tts_models: { repo_id: string; label: string; installed: boolean; incomplete: boolean; supported: boolean; engines: string[] }[];
  voices: { voice_id: string; name: string; type: string; language: string | null }[];
  voice_alias_count: number;
  stt_models: string[];
  dictation_engine_available: boolean;
  dictation_models: { vision?:boolean|null; id: string; label: string; languages: string; installed: boolean }[];
}

