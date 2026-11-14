<script lang="ts">
  import { onMount } from 'svelte'
  import { nativeBridge, nativeSession, type AssistantAction } from './bridge'
  import { parseAssistant, parseAssistantState, parseChoices, type AssistantScope, type AssistantSettings } from './assistant'
  import { notify } from '../../../../web/estompe/src/lib/ui'

  const bridge = nativeBridge()
  let s = $state<AssistantSettings>(parseAssistant(bridge?.assistantSettings?.() ?? ''))
  let st = $state(parseAssistantState(bridge?.assistantState?.() ?? ''))
  let model = $state(bridge?.driveModel() ?? '')
  let apps = $state(parseChoices('[]'))
  let musicApps = $state(parseChoices('[]'))

  async function ask(action: AssistantAction, settings?: AssistantSettings): Promise<string | undefined> {
    const r = await nativeSession()?.({ op: 'assistant', action, settings })
    return r?.status === 200 ? (r.value ?? '') : undefined
  }

  async function save(patch: Partial<AssistantSettings>) {
    s = { ...s, ...patch }
    const why = await ask('save', $state.snapshot(s))
    if (why === '') return
    notify.info(why ?? 'Réglages non enregistrés.')
    s = parseAssistant(bridge?.assistantSettings?.() ?? '')
  }

  function toggleApp(id: string, on: boolean) {
    save({ apps: on ? [...s.apps.filter((a) => a !== id), id] : s.apps.filter((a) => a !== id) })
  }

  function downloadModel() {
    bridge?.driveDownloadModel()
    model = 'busy'
  }

  function downloadLlm() {
    ask('downloadLlm')
    st = { ...st, llm: 'busy' }
  }

  async function loadApps() {
    apps = parseChoices((await ask('apps')) ?? '[]')
    musicApps = parseChoices((await ask('musicApps')) ?? '[]')
  }

  onMount(() => {
    loadApps()
    const t = setInterval(() => {
      if (!bridge) return
      const next = parseAssistantState(bridge.assistantState?.() ?? '')
      const changed = next.notifications !== st.notifications
      st = next
      model = bridge.driveModel()
      if (changed) loadApps()
    }, 1500)
    return () => clearInterval(t)
  })
</script>

<h3 class="set-sub">Assistant vocal</h3>
<p class="v-note">
  Réveillé par le mot « Estompe », il écoute et comprend sur le téléphone : rien ne part vers un serveur.
  Essaie « Estompe, lis mes messages », « Estompe, envoie un SMS à Marie », « Estompe, mets Daft Punk » ou « Estompe, lance YouTube Music ».
</p>

<div class="plbls-field">
  <label class="plbls-field__label" for="assistant-scope">Écoute</label>
  <select id="assistant-scope" class="plbls-select" value={s.scope} onchange={(e) => save({ scope: e.currentTarget.value as AssistantScope })}>
    <option value="off">Éteint</option>
    <option value="drive">En mode conduite seulement</option>
    <option value="always">Tout le temps</option>
  </select>
  {#if s.scope === 'always'}
    <p class="v-note">
      L'écoute permanente consomme de la batterie et garde une notification affichée. Après un redémarrage du téléphone, rouvre Estompe pour relancer l'écoute.
    </p>
  {/if}
  {#if s.scope !== 'off'}
    <p class="v-note">
      Attention : l'assistant répond même téléphone verrouillé. Toute personne à portée de voix peut faire lire tes messages ou dicter un SMS.
      Un SMS n'est jamais envoyé sans que l'assistant te le relise et que tu dises « envoyer ».
    </p>
    <p class="v-note">{st.listening ? 'À l\'écoute.' : s.scope === 'drive' ? 'En attente : l\'écoute démarre avec le mode conduite.' : 'Pas encore à l\'écoute : vérifie les autorisations et le modèle de dictée ci-dessous.'}</p>
  {/if}
</div>

<div class="plbls-field">
  <span class="plbls-field__label">Déclenchement par bouton</span>
  <p class="v-note">
    Choisis Estompe comme assistant numérique du téléphone : un appui long sur le bouton d'alimentation ou d'accueil, le geste d'assistant,
    ou le bouton de commande vocale du volant ou d'un casque Bluetooth lance l'écoute d'un ordre, sans dire « Estompe ».
    Le micro n'est ouvert que le temps de l'ordre : aucune consommation le reste du temps, même avec l'écoute réglée sur « Éteint ».
  </p>
  <p class="v-note">{st.role ? 'Estompe est l\'assistant du téléphone.' : 'Estompe n\'est pas l\'assistant du téléphone.'}</p>
  {#if !st.role}<button class="plbls-button plbls-button--compact" onclick={() => bridge?.assistantGrant?.('role')}>Choisir l'assistant du téléphone</button>{/if}
</div>

<div class="plbls-field">
  <span class="plbls-field__label">Autorisations</span>
  <p class="v-note">Micro : {st.mic ? 'accordé.' : 'manquant.'}</p>
  {#if !st.mic}<button class="plbls-button plbls-button--compact" onclick={() => bridge?.assistantGrant?.('mic')}>Autoriser le micro</button>{/if}
  <p class="v-note">SMS et contacts : {st.sms && st.contacts ? 'accordés.' : 'manquants.'}</p>
  {#if !st.sms || !st.contacts}<button class="plbls-button plbls-button--compact" onclick={() => bridge?.assistantGrant?.('sms')}>Autoriser les SMS et les contacts</button>{/if}
  <p class="v-note">Accès aux notifications (pour lire les autres messageries et piloter la musique) : {st.notifications ? 'accordé.' : 'manquant.'}</p>
  {#if !st.notifications}<button class="plbls-button plbls-button--compact" onclick={() => bridge?.assistantGrant?.('notifications')}>Ouvrir le réglage des notifications</button>{/if}
  <p class="v-note">Affichage par-dessus les autres applications (bandeau « J'écoute… » et ce qui a été compris, ouverture de l'application de musique depuis l'arrière-plan) : {st.overlay ? 'accordé.' : 'manquant.'}</p>
  {#if !st.overlay}<button class="plbls-button plbls-button--compact" onclick={() => bridge?.assistantGrant?.('overlay')}>Autoriser l'affichage par-dessus</button>{/if}
  <p class="v-note">Batterie sans restriction (pour que l'écoute continue écran éteint ou Estompe en arrière-plan) : {st.battery ? 'accordé.' : 'manquant.'}</p>
  {#if !st.battery}<button class="plbls-button plbls-button--compact" onclick={() => bridge?.assistantGrant?.('battery')}>Lever la restriction de batterie</button>{/if}
</div>

<div class="plbls-field">
  <span class="plbls-field__label">Dictée des ordres</span>
  {#if model === 'ready'}
    <p class="v-note">Modèle français installé.</p>
  {:else if model === 'busy'}
    <p class="v-note">Téléchargement du modèle (~40 Mo)…</p>
  {:else}
    {#if model}<p class="v-note">{model}</p>{/if}
    <button class="plbls-button plbls-button--compact" onclick={downloadModel}>Télécharger le modèle français (~40 Mo)</button>
  {/if}
</div>

<div class="plbls-field">
  <span class="plbls-field__label">Applications lues</span>
  <p class="v-note">Les SMS sont toujours lus si l'autorisation est accordée.</p>
  {#each apps as a (a.id)}
    <label class="plbls-checkbox">
      <input class="plbls-checkbox__input" type="checkbox" checked={s.apps.includes(a.id)} onchange={(e) => toggleApp(a.id, e.currentTarget.checked)} />
      <span class="plbls-checkbox__case"></span>
      <span class="plbls-checkbox__texte">{a.label}</span>
    </label>
  {:else}
    <p class="v-note">Aucune application détectée : accorde l'accès aux notifications.</p>
  {/each}
</div>

<div class="plbls-field">
  <label class="plbls-field__label" for="assistant-music">Application de musique par défaut</label>
  <select id="assistant-music" class="plbls-select" value={s.musicApp} onchange={(e) => save({ musicApp: e.currentTarget.value })}>
    <option value="">La dernière utilisée</option>
    {#each musicApps as m (m.id)}<option value={m.id}>{m.label}</option>{/each}
  </select>
</div>

{#if st.llmAvailable}
  <div class="plbls-field">
    <span class="plbls-field__label">Compréhension étendue (IA locale)</span>
    <label class="plbls-toggle">
      <input class="plbls-toggle__input" type="checkbox" checked={s.llm} onchange={(e) => save({ llm: e.currentTarget.checked })} />
      <span class="plbls-toggle__piste"></span>
      <span class="plbls-toggle__texte">Activer la compréhension étendue</span>
    </label>
    {#if st.llm === 'ready'}
      <p class="v-note">Modèle installé.</p>
      <button class="plbls-button plbls-button--compact" onclick={() => ask('deleteLlm')}>Supprimer le modèle</button>
    {:else if st.llm === 'busy'}
      <p class="v-note">Téléchargement du modèle…</p>
    {:else}
      <p class="v-note">{st.llm || 'Modèle absent.'}</p>
      <button class="plbls-button plbls-button--compact" onclick={downloadLlm}>Télécharger le modèle</button>
    {/if}
    <p class="v-note">
      Facultatif : utilisée seulement quand aucun ordre n'est reconnu. Le modèle pèse plusieurs centaines de Mo ; rien ne quitte le téléphone.
    </p>
  </div>
{/if}
