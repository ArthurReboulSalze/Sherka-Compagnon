export type Emotion = 'neutral' | 'joy' | 'sadness' | 'anger' | 'surprise' | 'reflection' | 'tenderness';
export type Theme = 'gold' | 'sage' | 'ocean' | 'iris' | 'rose' | 'mono';
export interface IllustrationConfig { enabled:boolean; server_url:string; checkpoint:string|null; model_family:'illustrious'|'sdxl'; checkpoint_folder:string; lora_folder:string; negative_prompt:string; physical_description:string; loras:{name:string;strength:number}[]; turbo:boolean; image_count:number; slide_seconds:number; workflow:Record<string,unknown>|null }
export interface IllustrationCatalog { checkpoints:string[]; loras:string[]; turbo_available:boolean; missing_nodes:string[] }
export interface IllustrationStatus { profile_id:string|null; phase:'disabled'|'idle'|'planning'|'generating'|'ready'|'error'; image:string|null; images:string[]; message:string|null }
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
  | { kind: 'theme'; theme: Theme }
  | { kind: 'illustration'; id:string; revision:number; config:IllustrationConfig }
  | { kind: 'web_search'; mode: SearchMode; id?: string; revision?: number;config?:SearchConfig }
  | { kind: 'interface_language'; language: import('./i18n').Language }
  | { kind: 'create_profile'; draft: ProfileDraft }
  | { kind: 'update_profile'; id: string; revision: number; draft: ProfileDraft }
  | { kind: 'switch_profile' | 'duplicate_profile' | 'delete_profile'; id: string }
  | { kind: 'set_avatar'; id: string; revision: number; emotion?: Emotion; base64: string | null }
  | { kind: 'import_profile'; json: string }
  | { kind: 'preferences'; preferences: { always_on_top: boolean; widget_scale: number } }
  | { kind: 'connections'; id: string; revision: number; connections: Connections }
  | { kind: 'audio_settings'; audio: AudioSettings }
  | { kind: 'pause_voice' | 'stop_response' };

export type WidgetAction = 'stop_hermes' | 'show_panel' | 'show_widget' | 'hide_widget' | 'toggle_widget' | 'click_through' | 'restore_widget' | 'stop_response' | 'pause_voice' | 'toggle_microphone' | 'quit' | 'quit_without_unloading';

export interface AudioSettings { input_device: string | null; output_device: string | null; speakers: boolean; silence_ms: number; onset_ms: number; interruption_ms: number }
export interface ContextBudget { model: string; max_context: number | null; loaded_context: number; limit: number; input_estimate: number; response_tokens: number; safety_tokens: number; history_kept: number; history_omitted: number; memories_used: number[]; memories_omitted: number }
export type SearchMode = 'off' | 'on_request' | 'auto';
export interface WebReport { query: string; searching: boolean; error: string | null; sources: {title:string;url:string;snippet:string}[] }
export interface LiveConversation { emotion: Emotion; hermes_approval?: {run_id:string;request_id:string|null;description:string;allow_once:boolean}|null; web: WebReport | null; tag: { profile_id: string; session_id: string; turn_id: string; generation_id: string } | null; transcript: string; reply: string; heard: string; error: string | null; notice: string | null; budget: ContextBudget | null; memory_receipt: string | null; usage: {length_limited:boolean;prompt_tokens:number|null;completion_tokens:number|null;reasoning_tokens:number|null}|null }
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

