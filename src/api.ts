import { invoke, isTauri } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { Message, Mutation, Snapshot, WidgetAction, LanguageCatalog, VoiceCatalog, AudioDevices, LiveState } from './types';
import type { MemoryState, MemoryEdit, MemorySource } from './types';

export const desktopAvailable = () => isTauri();
export const getStartupIssue = () => invoke<{language:string;error:string;data_directory:string} | null>('get_startup_issue');
export const closeStartupRecovery = () => invoke<void>('close_startup_recovery');
export const getSnapshot = () => invoke<Snapshot>('get_snapshot');
export const mutate = (mutation: Mutation) => invoke<Snapshot>('apply_mutation', { mutation });
export const history = (profileId: string) => invoke<Message[]>('get_history', { profileId });
export const memories = (profileId: string, query = '') => invoke<MemoryState>('get_memories', { profileId, query });
export const editMemory = (profileId: string, epoch: number, edit: MemoryEdit) => invoke<MemoryState>('edit_memory', { profileId, epoch, edit });
export const searchArchive = (profileId: string, query: string) => invoke<MemorySource[]>('search_archive', { profileId, query });
export const exportMemories = (profileId: string) => invoke<boolean>('export_memories', { profileId });
export const onMemory = (callback: (id: string) => void) => listen<string>('sherka:memory', e => callback(e.payload));
export const exportProfile = (profileId: string) => invoke<boolean>('export_profile', { profileId });
export const widgetAction = (action: WidgetAction) => invoke<void>('widget_action', { action });
export const widgetVisibility = () => invoke<boolean>('get_widget_visibility');
export const onWidgetVisibility = (callback: (visible: boolean) => void) => listen<boolean>('sherka:widget-visibility', e => callback(e.payload));
export const switchCompanion = (id: string) => invoke<Snapshot>('switch_companion', { id });
export const onQuit = (callback: (value: { stage: 'unloading' | 'failed'; error?: string }) => void) => listen<{ stage: 'unloading' | 'failed'; error?: string }>('sherka:quit', e => callback(e.payload));
export const onChanged = (callback: () => void) => listen('sherka:changed', callback);
export const discoverLanguageModels = (baseUrl: string) => invoke<LanguageCatalog>('discover_language_models', { baseUrl });
export const discoverVoices = (baseUrl: string) => invoke<VoiceCatalog>('discover_voices', { baseUrl });
export const audioDevices = () => invoke<AudioDevices>('get_audio_devices');
export const conversation = () => invoke<LiveState>('get_conversation');
export const sendMessage = (profileId: string, text: string, speak: boolean) => invoke<void>('send_message', { profileId, text, speak });
export const openWebSource = (profileId: string, messageId: string | null, index: number) => invoke<void>('open_web_source', { profileId, messageId, index });
export const onLive = (callback: (state: LiveState) => void) => listen<LiveState>('sherka:live', (e) => callback(e.payload));
export const onHistory = (callback: (profileId: string) => void) => listen<string>('sherka:history', (e) => callback(e.payload));
export const onMeter = (callback: (value: { level: number; speech: boolean }) => void) => listen<{ level: number; speech: boolean }>('sherka:meter', (e) => callback(e.payload));


export interface VoiceService { attempt: number; url: string; phase: 'disabled' | 'checking' | 'starting' | 'ready' | 'error'; message: string }
export const getVoiceService = () => invoke<VoiceService>('get_voice_service');
export const retryVoiceService = () => invoke<void>('retry_voice_service');
export const onVoiceService = (callback: (value: VoiceService) => void) => listen<VoiceService>('sherka:voice-service', event => callback(event.payload));

export interface UpdateStatus { current_version: string; mode: 'installed' | 'portable'; phase: 'idle' | 'checking' | 'available' | 'current' | 'downloading' | 'installing' | 'error'; version?: string; notes?: string; downloaded: number; total?: number; message: string }
export const getUpdateStatus = () => invoke<UpdateStatus>('get_update_status');
export const checkUpdate = () => invoke<UpdateStatus>('check_update');
export const installUpdate = () => invoke<UpdateStatus>('install_update');
export const onUpdate = (callback: (value: UpdateStatus) => void) => listen<UpdateStatus>('sherka:update', event => callback(event.payload));
