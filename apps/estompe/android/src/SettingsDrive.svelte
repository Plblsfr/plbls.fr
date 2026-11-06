<script lang="ts">
  import { onMount } from 'svelte'
  import { nativeBridge } from './bridge'
  import { parseChoices, parseDrive, type DriveSettings } from './drive'
  import { debug } from '../../../../web/estompe/src/lib/debuglog.svelte'
  import { notify } from '../../../../web/estompe/src/lib/ui'

  const bridge = nativeBridge()
  let s = $state<DriveSettings>(parseDrive(bridge?.driveSettings() ?? ''))
  let active = $state(bridge?.driveActive() ?? false)
  let model = $state(bridge?.driveModel() ?? '')
  const engines = parseChoices(bridge?.driveEngines() ?? '[]')
  let cars = $state(parseChoices(bridge?.driveBluetooth() ?? '[]'))

  function save(patch: Partial<DriveSettings>) {
    s = { ...s, ...patch }
    bridge?.saveDriveSettings(JSON.stringify($state.snapshot(s)))
  }

  function toggle() {
    if (active) bridge?.driveStop()
    else {
      const why = bridge?.driveStart()
      if (why) return notify.info(why)
    }
    active = !active
  }

  function download() {
    bridge?.driveDownloadModel()
    model = 'busy'
  }

  // le téléchargement du modèle (~40 Mo) et les erreurs de voix se suivent ici et dans le mode debug
  onMount(() => {
    let seen = ''
    const t = setInterval(() => {
      if (!bridge) return
      model = bridge.driveModel()
      active = bridge.driveActive()
      if (cars.length === 0) cars = parseChoices(bridge.driveBluetooth())
      const log = bridge.driveLog()
      if (log !== seen) {
        log.split('\n').slice(seen ? seen.split('\n').length : 0).filter(Boolean).forEach((l) => debug.log('warn', 'conduite', l))
        seen = log
      }
    }, 1500)
    return () => clearInterval(t)
  })
</script>

<h3 class="set-sub">Mode conduite</h3>
<p class="muted small">
  Lit à voix haute les messages privés et les mentions, puis te laisse répondre en dictant. La voix et la dictée restent sur le téléphone, sans service Google.
  Raccourci : tuile « Mode conduite » des réglages rapides d'Android.
</p>

<button class="plbls-button" onclick={toggle}>{active ? 'Arrêter le mode conduite' : 'Démarrer le mode conduite'}</button>

<div class="plbls-field">
  <span class="plbls-field__label">Dictée des réponses</span>
  {#if model === 'ready'}
    <p class="muted small">Modèle français installé.</p>
  {:else if model === 'busy'}
    <p class="muted small">Téléchargement du modèle (~40 Mo)…</p>
  {:else}
    {#if model}<p class="muted small">{model}</p>{/if}
    <button class="plbls-button plbls-button--compact" onclick={download}>Télécharger le modèle français (~40 Mo)</button>
  {/if}
</div>

<div class="plbls-field">
  <label class="plbls-field__label" for="drive-engine">Moteur de synthèse vocale</label>
  <select id="drive-engine" class="plbls-select" value={s.engine} onchange={(e) => save({ engine: e.currentTarget.value })}>
    <option value="">Celui du système</option>
    {#each engines as e (e.id)}<option value={e.id}>{e.label}</option>{/each}
  </select>
  {#if engines.length === 0}<p class="muted small">Aucun moteur détecté : installe-en un (RHVoice, eSpeak…).</p>{/if}
</div>

<div class="plbls-field">
  <label class="plbls-field__label" for="drive-rate">Vitesse de la voix ({s.rate.toFixed(1)}×)</label>
  <input id="drive-rate" type="range" min="0.5" max="2" step="0.1" value={s.rate} onchange={(e) => save({ rate: Number(e.currentTarget.value) })} />
</div>

<label class="plbls-toggle">
  <input class="plbls-toggle__input" type="checkbox" checked={s.readContent} onchange={(e) => save({ readContent: e.currentTarget.checked })} />
  <span class="plbls-toggle__piste"></span>
  <span class="plbls-toggle__texte">Lire le contenu (sinon seulement l'expéditeur)</span>
</label>

<label class="plbls-toggle">
  <input class="plbls-toggle__input" type="checkbox" checked={s.confirm} onchange={(e) => save({ confirm: e.currentTarget.checked })} />
  <span class="plbls-toggle__piste"></span>
  <span class="plbls-toggle__texte">Confirmer avant d'envoyer (sinon envoi automatique après quelques secondes)</span>
</label>

<label class="plbls-toggle">
  <input class="plbls-toggle__input" type="checkbox" checked={s.autoBluetooth} onchange={(e) => save({ autoBluetooth: e.currentTarget.checked })} />
  <span class="plbls-toggle__piste"></span>
  <span class="plbls-toggle__texte">Activer automatiquement à la connexion Bluetooth de la voiture</span>
</label>
{#if s.autoBluetooth}
  <div class="plbls-field">
    <label class="plbls-field__label" for="drive-car">Appareil Bluetooth</label>
    <select id="drive-car" class="plbls-select" value={s.bluetooth} onchange={(e) => save({ bluetooth: e.currentTarget.value })}>
      <option value="">Choisir…</option>
      {#each cars as c (c.id)}<option value={c.id}>{c.label}</option>{/each}
    </select>
  </div>
{/if}
