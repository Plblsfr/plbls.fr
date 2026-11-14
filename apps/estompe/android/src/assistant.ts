import { parseChoices, type Choice } from './drive'

export type AssistantScope = 'off' | 'drive' | 'always'
export type AssistantSettings = { scope: AssistantScope; apps: string[]; musicApp: string; llm: boolean; announce: boolean }
export type AssistantState = { listening: boolean; mic: boolean; notifications: boolean; sms: boolean; contacts: boolean; overlay: boolean; battery: boolean; role: boolean; debug: boolean; record: boolean; recordings: string; model: string; llm: string; llmAvailable: boolean }

export const ASSISTANT_DEFAULTS: AssistantSettings = { scope: 'off', apps: [], musicApp: '', llm: false, announce: false }
export const ASSISTANT_STATE_DEFAULTS: AssistantState = { listening: false, mic: false, notifications: false, sms: false, contacts: false, overlay: false, battery: false, role: false, debug: false, record: false, recordings: '', model: '', llm: '', llmAvailable: false }

function read(json: string): Record<string, unknown> {
  try {
    const j: unknown = JSON.parse(json)
    return j && typeof j === 'object' && !Array.isArray(j) ? (j as Record<string, unknown>) : {}
  } catch {
    return {}
  }
}

export function parseAssistant(json: string): AssistantSettings {
  const j = read(json)
  const d = ASSISTANT_DEFAULTS
  const scope: AssistantScope = j.scope === 'drive' || j.scope === 'always' ? j.scope : d.scope
  return {
    scope, apps: Array.isArray(j.apps) ? j.apps.filter((a): a is string => typeof a === 'string') : [],
    musicApp: typeof j.musicApp === 'string' ? j.musicApp : d.musicApp, llm: typeof j.llm === 'boolean' ? j.llm : d.llm, announce: typeof j.announce === 'boolean' ? j.announce : d.announce,
  }
}

export function parseAssistantState(json: string): AssistantState {
  const j = read(json)
  const d = ASSISTANT_STATE_DEFAULTS
  const bool = (k: keyof AssistantState, def: boolean) => (typeof j[k] === 'boolean' ? (j[k] as boolean) : def)
  const str = (k: keyof AssistantState, def: string) => (typeof j[k] === 'string' ? (j[k] as string) : def)
  return {
    listening: bool('listening', d.listening), mic: bool('mic', d.mic), notifications: bool('notifications', d.notifications), sms: bool('sms', d.sms),
    contacts: bool('contacts', d.contacts), overlay: bool('overlay', d.overlay), battery: bool('battery', d.battery), role: bool('role', d.role), debug: bool('debug', d.debug), record: bool('record', d.record), recordings: str('recordings', d.recordings), model: str('model', d.model), llm: str('llm', d.llm), llmAvailable: bool('llmAvailable', d.llmAvailable),
  }
}

export { parseChoices, type Choice }
