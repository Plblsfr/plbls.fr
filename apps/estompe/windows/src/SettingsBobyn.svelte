<script lang="ts">
  import { onMount } from 'svelte'
  import { audioConfig, audioRows, bobynApply, bobynHotkey, bobynSave, bobynState, micFromPrefs, type AudioRow, type BobynConfig, type BobynMachine, type BobynStatus } from './bobyn'
  import { notify } from '../../../../web/estompe/src/lib/ui'
  import { OVERLAY_POS } from './overlay'
  import { prefs } from '../../../../web/estompe/src/lib/prefs.svelte'

  let cfg = $state<BobynConfig>()
  let machine = $state<BobynMachine>()
  let status = $state<BobynStatus | null>(null)
  let rows = $state<AudioRow[]>([])
  let mic = $state('')
  let ptt = $state('')
  let newGame = $state('')
  let busy = $state(false)
  /** Raccourci en cours de saisie : on attend la prochaine combinaison. */
  let capturing = $state<'hotkey' | 'hotkey_last' | 'hotkey_marker' | ''>('')
  let error = $state('')

  const CODECS: Record<string, string> = { h264: 'H.264', hevc: 'HEVC', av1: 'AV1' }
  const MODES = [['game', 'Jeu au premier plan'], ['window', 'Fenêtre précise'], ['screen', 'Écran']] as const

  onMount(() => {
    let off = () => {}
    void (async () => {
      try {
        const s = await bobynState()
        cfg = s.config
        machine = s.machine
        status = s.status
        error = s.error ?? ''
        rows = audioRows(s.config.audio, s.machine.audio_apps)
        const m = s.config.audio.find((a) => a.source === 'mic')
        mic = m ? (m.device ?? 'default') : ''
        ptt = m?.ptt ?? ''
        off = await (await import('@tauri-apps/api/event')).listen<BobynStatus>('bobyn-status', (e) => (status = e.payload))
      } catch (e) {
        error = String(e)
      }
    })()
    return () => off()
  })

  async function apply() {
    if (!cfg) return
    busy = true
    try {
      if (mic === 'prefs') {
        const label = (await navigator.mediaDevices.enumerateDevices().catch(() => [])).find((d) => d.kind === 'audioinput' && d.deviceId === prefs.micId)?.label ?? ''
        const id = micFromPrefs(label, machine?.mics ?? [])
        if (!id) notify.info("Micro des Paramètres introuvable : micro par défaut utilisé.")
        mic = id ?? 'default'
      }
      cfg.audio = audioConfig(rows, mic, ptt)
      await bobynApply($state.snapshot(cfg))
      notify.info('Réglages Bobyn appliqués.')
    } catch (e) {
      notify.info(`Bobyn : ${e}`)
    } finally {
      busy = false
    }
  }

  async function saveNow(last: boolean) {
    try { await bobynSave(last) } catch (e) { notify.info(`Bobyn : ${e}`) }
  }

  function captureKey(e: KeyboardEvent) {
    e.preventDefault()
    if (!cfg || !capturing) return
    if (e.code === 'Escape') { capturing = ''; return }
    if (e.code === 'Backspace' && capturing !== 'hotkey') { cfg[capturing] = null; capturing = ''; return }
    const key = bobynHotkey(e)
    if (!key) return
    cfg[capturing] = key
    capturing = ''
  }

  function addGame() {
    const g = newGame.trim()
    if (cfg && g && !cfg.games.some((x) => x.toLowerCase() === g.toLowerCase())) cfg.games.push(g)
    newGame = ''
  }
</script>

<p class="muted small">
  Fonction alpha, Windows seulement : Estompe garde en mémoire les dernières secondes de jeu (image du GPU et son), et les enregistre en MP4 sur un raccourci.
  Rien n'est capturé tant que Bobyn est désactivé.
</p>

{#if error}<p class="small" style="color: var(--danger)">{error}</p>{/if}

{#if cfg && machine}
  <label class="plbls-toggle">
    <input class="plbls-toggle__input" type="checkbox" bind:checked={cfg.enabled} onchange={apply} disabled={busy} />
    <span class="plbls-toggle__piste"></span>
    <span class="plbls-toggle__texte">Activer Bobyn</span>
  </label>
  {#if status}<p class="small" role="status">{status.text}</p>{/if}
  <div class="row">
    <select class="plbls-select" aria-label="Pastille Bobyn sur le jeu" bind:value={prefs.bobynOverlay} onchange={() => prefs.save()}>
      <option value="off">Pastille sur le jeu : masquée</option>
      {#each Object.entries(OVERLAY_POS) as [k, v] (k)}<option value={k}>Pastille sur le jeu : {v.toLowerCase()}</option>{/each}
    </select>
  </div>
  <div class="row">
    <button class="plbls-button plbls-button--compact plbls-button--contour" disabled={!cfg.enabled || status?.state !== 'recording'} onclick={() => saveNow(false)}>Sauvegarder le replay</button>
    <button class="plbls-button plbls-button--compact plbls-button--contour" disabled={!cfg.enabled || status?.state !== 'recording'} onclick={() => saveNow(true)}>Sauvegarder les dernières secondes</button>
  </div>

  <strong>Ce qui est enregistré</strong>
  <div class="plbls-field">
    <label class="plbls-field__label" for="bb-mode">Source</label>
    <select id="bb-mode" class="plbls-select" bind:value={cfg.mode}>
      {#each MODES as [id, label] (id)}<option value={id}>{label}</option>{/each}
    </select>
  </div>
  {#if cfg.mode === 'window'}
    <div class="plbls-field">
      <label class="plbls-field__label" for="bb-window">Fenêtre</label>
      <select id="bb-window" class="plbls-select" bind:value={cfg.window}>
        {#if cfg.window && !machine.windows.some((w) => w.exe.toLowerCase() === cfg!.window.toLowerCase())}<option value={cfg.window}>{cfg.window} (non lancé)</option>{/if}
        {#each machine.windows as w (w.exe + w.title)}<option value={w.exe}>{w.exe} — {w.title}</option>{/each}
      </select>
    </div>
  {:else if cfg.mode === 'screen'}
    <div class="plbls-field">
      <label class="plbls-field__label" for="bb-screen">Écran</label>
      <select id="bb-screen" class="plbls-select" bind:value={cfg.screen}>
        {#each machine.monitors as m, i (m.name)}<option value={i}>{m.name.replace(/^\\\\\.\\/, '')} — {m.width}×{m.height}{m.primary ? ' (principal)' : ''}</option>{/each}
      </select>
    </div>
  {/if}
  <label class="plbls-toggle">
    <input class="plbls-toggle__input" type="checkbox" bind:checked={cfg.always_on} />
    <span class="plbls-toggle__piste"></span>
    <span class="plbls-toggle__texte">Buffer toujours actif</span>
  </label>
  <p class="muted small">Sans cela, le buffer démarre avec le jeu et s'arrête à sa fermeture. Une fenêtre réduite le met en pause.</p>

  <strong>Qualité</strong>
  <div class="plbls-field">
    <label class="plbls-field__label" for="bb-seconds">Durée conservée : {cfg.seconds} s</label>
    <input id="bb-seconds" type="range" min="15" max="600" step="5" bind:value={cfg.seconds} />
  </div>
  <div class="plbls-field">
    <label class="plbls-field__label" for="bb-height">Résolution</label>
    <select id="bb-height" class="plbls-select" bind:value={cfg.height}>
      <option value={null}>Native</option><option value={1440}>1440p</option><option value={1080}>1080p</option><option value={720}>720p</option>
    </select>
  </div>
  <div class="plbls-field">
    <label class="plbls-field__label" for="bb-fps">Images par seconde</label>
    <select id="bb-fps" class="plbls-select" bind:value={cfg.fps}>
      <option value={30}>30</option><option value={60}>60</option><option value={120}>120</option><option value={144}>144</option><option value={0}>Fréquence de l'écran</option>
    </select>
  </div>
  <div class="plbls-field">
    <label class="plbls-field__label" for="bb-codec">Codec</label>
    <select id="bb-codec" class="plbls-select" bind:value={cfg.codec}>
      {#each machine.codecs as c (c)}<option value={c}>{CODECS[c] ?? c}</option>{/each}
    </select>
    <span class="plbls-field__aide">Seuls les codecs que ta carte graphique sait encoder sont proposés.</span>
  </div>
  <div class="plbls-field">
    <label class="plbls-field__label" for="bb-mbps">Débit (Mb/s)</label>
    <input id="bb-mbps" class="plbls-input" type="number" min="1" max="200" bind:value={cfg.mbps} />
  </div>
  <label class="plbls-toggle">
    <input class="plbls-toggle__input" type="checkbox" checked={!!cfg.cursor} onchange={(e) => (cfg!.cursor = e.currentTarget.checked || null)} />
    <span class="plbls-toggle__piste"></span>
    <span class="plbls-toggle__texte">Afficher le curseur</span>
  </label>
  <div class="plbls-field">
    <label class="plbls-field__label" for="bb-dir">Dossier des replays</label>
    <input id="bb-dir" class="plbls-input" type="text" placeholder="Vidéos\Replays" value={cfg.dir ?? ''} oninput={(e) => (cfg!.dir = e.currentTarget.value.trim() || null)} />
  </div>

  <strong>Audio</strong>
  <p class="muted small">Chaque source est une piste séparée du fichier ; une piste « Mix » (tout le son, micro compris) s'ajoute dès qu'il y en a plusieurs. Le micro est enregistré brut, avec le volume choisi ci-dessus.</p>
  {#each rows as r (r.source)}
    <div class="row">
      <label class="plbls-checkbox">
        <input type="checkbox" class="plbls-checkbox__input" bind:checked={r.on} />
        <span class="plbls-checkbox__case" aria-hidden="true"></span>
        <span class="plbls-checkbox__texte truncate">{r.label}</span>
      </label>
      <input type="range" min="0" max="2" step="0.05" bind:value={r.gain} aria-label={`Volume de ${r.label}`} />
    </div>
  {/each}
  <div class="plbls-field">
    <label class="plbls-field__label" for="bb-mic">Micro</label>
    <select id="bb-mic" class="plbls-select" bind:value={mic}>
      <option value="">Désactivé</option><option value="default">Périphérique par défaut</option><option value="prefs">Celui choisi dans les Paramètres d'Estompe</option>
      {#each machine.mics as m (m.id)}<option value={m.id}>{m.name}</option>{/each}
    </select>
  </div>
  {#if mic}
    <div class="plbls-field">
      <label class="plbls-field__label" for="bb-ptt">Push-to-talk du micro</label>
      <input id="bb-ptt" class="plbls-input" type="text" placeholder="V, F9…" bind:value={ptt} />
      <span class="plbls-field__aide">Vide : le micro est toujours enregistré. Sinon, seulement touche maintenue.</span>
    </div>
  {/if}

  <strong>Raccourcis</strong>
  {#snippet hotkey(id: 'hotkey' | 'hotkey_last' | 'hotkey_marker', label: string, none: string)}
    <div class="plbls-field">
      <span class="plbls-field__label">{label}</span>
      <div class="row">
        {#if capturing === id}
          <input class="plbls-input" readonly placeholder={id !== 'hotkey' ? 'Appuie sur le raccourci… (Retour arrière : aucun)' : 'Appuie sur le raccourci…'} aria-label={label} onkeydown={captureKey} onblur={() => (capturing = '')} {@attach (n) => n.focus()} />
        {:else}
          <kbd>{cfg![id] ?? none}</kbd>
          <button class="plbls-button plbls-button--compact plbls-button--contour" onclick={() => (capturing = id)}>Changer</button>
        {/if}
      </div>
    </div>
  {/snippet}
  {@render hotkey('hotkey', 'Sauvegarder tout le buffer', 'Aucun')}
  {@render hotkey('hotkey_last', 'Sauvegarder les dernières secondes', 'Aucun')}
  {@render hotkey('hotkey_marker', 'Poser un marqueur dans l’enregistrement', 'Aucun')}
  <div class="plbls-field">
    <label class="plbls-field__label" for="bb-last">Durée des « dernières secondes »</label>
    <input id="bb-last" class="plbls-input" type="number" min="5" max="600" bind:value={cfg.last_seconds} />
  </div>
  <label class="plbls-toggle">
    <input class="plbls-toggle__input" type="checkbox" bind:checked={cfg.beep} />
    <span class="plbls-toggle__piste"></span>
    <span class="plbls-toggle__texte">Son de confirmation</span>
  </label>
  <label class="plbls-toggle">
    <input class="plbls-toggle__input" type="checkbox" bind:checked={cfg.marker_beep} />
    <span class="plbls-toggle__piste"></span>
    <span class="plbls-toggle__texte">Son à la pose d’un marqueur</span>
  </label>
  <label class="plbls-toggle">
    <input class="plbls-toggle__input" type="checkbox" bind:checked={cfg.marker_autosave} />
    <span class="plbls-toggle__piste"></span>
    <span class="plbls-toggle__texte">Sauvegarder le replay quand je pose un marqueur</span>
  </label>
  <label class="plbls-toggle">
    <input class="plbls-toggle__input" type="checkbox" bind:checked={cfg.notify} />
    <span class="plbls-toggle__piste"></span>
    <span class="plbls-toggle__texte">Notification Windows</span>
  </label>

  <strong>Jeux</strong>
  <p class="muted small">Ces exécutables sont toujours traités comme des jeux, même en fenêtré. La détection automatique reste active.</p>
  {#each cfg.games as g (g)}
    <div class="row">
      <span class="truncate">{g}</span>
      <button class="plbls-button plbls-button--compact plbls-button--discret" onclick={() => (cfg!.games = cfg!.games.filter((x) => x !== g))}>Retirer</button>
    </div>
  {/each}
  <div class="row">
    <input class="plbls-input" placeholder="Exécutable, ex. cod.exe" aria-label="Exécutable" bind:value={newGame} onkeydown={(e) => e.key === 'Enter' && addGame()} />
    <button class="plbls-button plbls-button--compact plbls-button--contour" onclick={addGame}>Ajouter</button>
  </div>

  <button class="plbls-button" class:is-busy={busy} disabled={busy} onclick={apply}>Appliquer</button>
{:else if !error}
  <p class="muted small">Chargement…</p>
{/if}
