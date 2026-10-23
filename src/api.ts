import { invoke, isTauri } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { IllustrationConfig, IllustrationStatus, IllustrationCatalog } from './types';
export const desktopTool = (tool:'text'|'image',open:boolean) => invoke<void>('desktop_tool',{tool,open});
export const widgetSurface = (surface?:import('./types').WidgetSurface,profileId?:string) => invoke<import('./types').WidgetSurface>('widget_surface',{surface:surface??null,profileId:profileId??null});
export const imageFullscreen = (open:boolean,profileId?:string) => invoke<void>('image_fullscreen',{open,profileId:profileId??null});
export const imageViewer = () => invoke<{profile_id:string;slide_seconds:number;ui_language:import('./i18n').Language;theme:import('./types').Theme}|null>('get_image_viewer');
export const onImageFullscreen = (callback:(open:boolean)=>void) => listen<boolean>('sherka:image-fullscreen',e=>callback(e.payload));
export const onAvatarSize=(callback:(size:[number,number])=>void)=>listen<[number,number]>('sherka:avatar-size',e=>callback(e.payload));
export const attachmentSupport = (profileId:string) => invoke<{images:boolean;known:boolean}>('get_attachment_support',{profileId});
export const searchKeyStatus=(provider:import('./types').SearchProvider)=>invoke<boolean>('search_key_status',{provider});
export const setSearchKey=(provider:import('./types').SearchProvider,key:string|null)=>invoke<void>('set_search_key',{provider,key});
export const testSearchConfig=(config:import('./types').SearchConfig,language:string)=>invoke<number>('test_search_config',{config,language});
export const comfyService=()=>invoke<import('./types').ComfyServiceStatus>('get_comfy_service');
export const onComfyService=(callback:(value:import('./types').ComfyServiceStatus)=>void)=>listen<import('./types').ComfyServiceStatus>('sherka:comfy-service',e=>callback(e.payload));
export const selectComfyPath=(python:boolean)=>invoke<string|null>('select_comfyui_path',{python});
export const illustration = () => invoke<IllustrationStatus>('get_illustration');
// Discovery depends only on the server and graph, not unfinished form values.
export const discoverIllustration = (config:IllustrationConfig,startIfNeeded=config.enabled) => invoke<IllustrationCatalog>('discover_illustration',{config:{server_url:config.server_url,workflow:config.workflow},startIfNeeded});
export const refreshIllustration = () => invoke<void>('refresh_illustration');
export const setIllustrationPrompt = (positive:string,negative:string) => invoke<void>('set_illustration_prompt',{positive,negative});
export const resetIllustrationPrompt = () => invoke<void>('reset_illustration_prompt');
export const onIllustration = (callback:(status:IllustrationStatus)=>void) => listen<IllustrationStatus>('sherka:illustration',e=>callback(e.payload));
export const characterStudio=()=>invoke<import('./types').CharacterStudioStatus>('get_character_studio');
export const onCharacterStudio=(callback:(status:import('./types').CharacterStudioStatus)=>void)=>listen<import('./types').CharacterStudioStatus>('sherka:character-studio',e=>callback(e.payload));
export const planCharacter=(sourceProfile:string,concept:string,language:import('./i18n').Language,config:import('./types').CharacterStudioConfig,image:string|null=null)=>invoke<void>('plan_character',{sourceProfile,concept,language,config,image});
export const importCharacterNeutral=(image:string)=>invoke<void>('import_character_neutral',{image});
export const editCharacterImages=(profileId:string,config:import('./types').CharacterStudioConfig)=>invoke<void>('edit_character_images',{profileId,config});
export const characterPrompts=(draft:import('./types').CharacterDraft,config:import('./types').CharacterStudioConfig)=>invoke<Record<import('./types').Emotion,import('./types').ScenePrompts>>('get_character_prompts',{draft,config});
export const generateCharacter=(draft:import('./types').CharacterDraft,config:import('./types').CharacterStudioConfig,emotion:import('./types').Emotion|null)=>invoke<void>('generate_character',{draft,config,emotion});
export const validateCharacterNeutral=(revision:number)=>invoke<void>('validate_character_neutral',{revision});
export const finishCharacter=(revision:number,draft:import('./types').CharacterDraft,config:import('./types').CharacterStudioConfig)=>invoke<string>('finish_character',{revision,draft,config});
export const discoverCharacterModels=(serverUrl:string)=>invoke<import('./types').CharacterStudioCatalog>('discover_character_models',{serverUrl});
export const characterWorkflow=(stage:import('./types').CharacterStage)=>invoke<Record<string,unknown>>('get_character_workflow',{stage});
export const processCharacterTransparency=(draft:import('./types').CharacterDraft,config:import('./types').CharacterStudioConfig)=>invoke<void>('process_character_transparency',{draft,config});
export const cancelCharacterStudio=()=>invoke<void>('cancel_character_studio');
import type { Message, Mutation, Snapshot, WidgetAction, LanguageCatalog, VoiceCatalog, AudioDevices, LiveState, Connections } from './types';
import type { MemoryState, MemoryEdit, MemorySource } from './types';

export const desktopAvailable = () => isTauri();
export const getStartupIssue = () => invoke<{language:string;error:string;data_directory:string} | null>('get_startup_issue');
export const closeStartupRecovery = () => invoke<void>('close_startup_recovery');
export const getSnapshot = () => invoke<Snapshot>('get_snapshot');
export const mutate = (mutation: Mutation) => invoke<Snapshot>('apply_mutation', { mutation });
export const history = (profileId: string) => invoke<Message[]>('get_history', { profileId });
export const clearConversation = (profileId: string, eraseSavedFacts = false, eraseMood = false) => invoke<void>('clear_conversation', { profileId, eraseSavedFacts, eraseMood });
export const onCleared = (callback: (id: string) => void) => listen<string>('sherka:cleared', e => callback(e.payload));
export const memories = (profileId: string, query = '') => invoke<MemoryState>('get_memories', { profileId, query });
export const editMemory = (profileId: string, epoch: number, edit: MemoryEdit) => invoke<MemoryState>('edit_memory', { profileId, epoch, edit });
export const searchArchive = (profileId: string, query: string) => invoke<MemorySource[]>('search_archive', { profileId, query });
export const exportMemories = (profileId: string) => invoke<boolean>('export_memories', { profileId });
export const optimizeMemory = (profileId:string) => invoke<import('./types').MemoryOptimization>('optimize_memory',{profileId});
export const memoryOptimization = (profileId:string) => invoke<import('./types').MemoryOptimization|null>('get_memory_optimization',{profileId});
export const finishMemoryOptimization = (profileId:string,apply:boolean) => invoke<void>('finish_memory_optimization',{profileId,apply});
export const onMemory = (callback: (id: string) => void) => listen<string>('sherka:memory', e => callback(e.payload));
export const exportProfile = (profileId: string) => invoke<boolean>('export_profile', { profileId });
export const widgetAction = (action: WidgetAction) => invoke<void>('widget_action', { action });
export const widgetVisibility = () => invoke<boolean>('get_widget_visibility');
export const onWidgetVisibility = (callback: (visible: boolean) => void) => listen<boolean>('sherka:widget-visibility', e => callback(e.payload));
export const switchCompanion = (id: string) => invoke<Snapshot>('switch_companion', { id });
export const onQuit = (callback: (value: { stage: 'unloading' | 'failed'; error?: string }) => void) => listen<{ stage: 'unloading' | 'failed'; error?: string }>('sherka:quit', e => callback(e.payload));
export const onChanged = (callback: () => void) => listen('sherka:changed', callback);
export const discoverLanguageModels = (config: string | Connections) => invoke<LanguageCatalog>('discover_language_models', typeof config === 'string' ? { baseUrl: config } : { connections: config });
export const providerKeyStatus = (connections: Connections) => invoke<boolean>('get_provider_key_status', { connections });
export const hermesProfiles = (server: string) => invoke<string[]>('get_hermes_profiles', { server });
export const hermesServers = () => invoke<string[]>('get_hermes_servers');
export const testHermes = (connections: Connections) => invoke<string[]>('test_hermes', { connections });
export const answerHermesApproval = (runId: string, requestId: string | null, allow: boolean) => invoke<void>('answer_hermes_approval', { runId, requestId, allow });
export const setProviderKey = (connections: Connections, key: string | null) => invoke<void>('set_provider_key', { connections, key });
export const discoverVoices = (baseUrl: string) => invoke<VoiceCatalog>('discover_voices', { baseUrl });
export const audioDevices = () => invoke<AudioDevices>('get_audio_devices');
export const conversation = () => invoke<LiveState>('get_conversation');
export const sendMessage = (profileId: string, text: string, speak: boolean,attachments:import('./types').Attachment[]=[]) => invoke<void>('send_message', { profileId, text, speak,attachments });
export const openWebSource = (profileId: string, messageId: string | null, index: number) => invoke<void>('open_web_source', { profileId, messageId, index });
export const onLive = (callback: (state: LiveState) => void) => listen<LiveState>('sherka:live', (e) => callback(e.payload));
export const onHistory = (callback: (profileId: string) => void) => listen<string>('sherka:history', (e) => callback(e.payload));
export const onMeter = (callback: (value: { level: number; speech: boolean }) => void) => listen<{ level: number; speech: boolean }>('sherka:meter', (e) => callback(e.payload));


export interface VoiceService { attempt: number; url: string; phase: 'disabled' | 'checking' | 'starting' | 'ready' | 'error'; message: string }
export const getVoiceService = () => invoke<VoiceService>('get_voice_service');
export const retryVoiceService = () => invoke<void>('retry_voice_service');
export const onVoiceService = (callback: (value: VoiceService) => void) => listen<VoiceService>('sherka:voice-service', event => callback(event.payload));

export interface UpdateStatus { current_version: string; mode: 'installed' | 'portable' | 'linux'; phase: 'idle' | 'checking' | 'available' | 'current' | 'downloading' | 'installing' | 'manual' | 'error'; version?: string; notes?: string; downloaded: number; total?: number; message: string }
export const getUpdateStatus = () => invoke<UpdateStatus>('get_update_status');
export const checkUpdate = () => invoke<UpdateStatus>('check_update');
export const installUpdate = () => invoke<UpdateStatus>('install_update');
export const onUpdate = (callback: (value: UpdateStatus) => void) => listen<UpdateStatus>('sherka:update', event => callback(event.payload));

export const newHermesConversation = (profileId: string) => invoke<Snapshot>('new_hermes_conversation', { profileId });

export const exportIllustrationWorkflow=(config:IllustrationConfig,transparent=false)=>invoke<boolean>('export_illustration_workflow',{config,transparent});

export const saveWidgetCell=(profileId:string,image:import('./types').ImagePlacement|null,textHeight:number|null)=>invoke<void>('save_widget_cell',{profileId,image,textHeight});

export const initiativeActivity=()=>invoke<void>('initiative_activity');

export const getIllustrationWorkflow=(config:IllustrationConfig,transparent=false)=>invoke<Record<string,unknown>>('get_illustration_workflow',{config,transparent});
