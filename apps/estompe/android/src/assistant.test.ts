import { describe, it, expect } from 'vitest'
import { ASSISTANT_DEFAULTS, ASSISTANT_STATE_DEFAULTS, parseAssistant, parseAssistantState } from './assistant'

describe('parseAssistant', () => {
  it('lit les réglages du pont', () => {
    expect(parseAssistant('{"scope":"always","apps":["org.thoughtcrime.securesms","com.whatsapp"],"musicApp":"com.spotify.music","llm":true}')).toEqual({
      scope: 'always', apps: ['org.thoughtcrime.securesms', 'com.whatsapp'], musicApp: 'com.spotify.music', llm: true,
    })
    expect(parseAssistant('{"scope":"drive"}').scope).toBe('drive')
  })
  it('retombe sur les défauts si le JSON est invalide ou mal typé', () => {
    expect(parseAssistant('pas du json')).toEqual(ASSISTANT_DEFAULTS)
    expect(parseAssistant('')).toEqual(ASSISTANT_DEFAULTS)
    expect(parseAssistant('null')).toEqual(ASSISTANT_DEFAULTS)
    expect(parseAssistant('[]')).toEqual(ASSISTANT_DEFAULTS)
    expect(parseAssistant('{"scope":3,"apps":"x","musicApp":4,"llm":"oui"}')).toEqual(ASSISTANT_DEFAULTS)
  })
  it('ramène une portée inconnue à « off » et ne garde que les chaînes de apps', () => {
    expect(parseAssistant('{"scope":"partout"}').scope).toBe('off')
    expect(parseAssistant('{"apps":["a",1,null,"b",{}]}').apps).toEqual(['a', 'b'])
  })
})

describe('parseAssistantState', () => {
  it('lit l\'état du pont', () => {
    const s = parseAssistantState('{"listening":true,"mic":true,"notifications":false,"sms":true,"contacts":true,"model":"ready","llm":"busy","llmAvailable":true}')
    expect(s).toEqual({ listening: true, mic: true, notifications: false, sms: true, contacts: true, model: 'ready', llm: 'busy', llmAvailable: true })
  })
  it('garde un message d\'erreur de modèle tel quel', () => {
    expect(parseAssistantState('{"model":"Réseau indisponible","llm":"Disque plein"}')).toMatchObject({ model: 'Réseau indisponible', llm: 'Disque plein' })
  })
  it('retombe sur les défauts si le JSON est invalide ou mal typé', () => {
    expect(parseAssistantState('x')).toEqual(ASSISTANT_STATE_DEFAULTS)
    expect(parseAssistantState('null')).toEqual(ASSISTANT_STATE_DEFAULTS)
    expect(parseAssistantState('{"listening":1,"mic":"oui","model":2,"llm":null,"llmAvailable":"x"}')).toEqual(ASSISTANT_STATE_DEFAULTS)
  })
})
