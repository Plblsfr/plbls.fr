<script lang="ts">
  import { onMount } from 'svelte'
  import Modal from '../../components/Modal.svelte'
  import Icon from '../../components/Icon.svelte'
  import { api } from '../../lib/api'
  import { platform } from '../../lib/platform'
  import { debug } from '../../lib/debuglog.svelte'
  import { MAX_SHOTS, REPORT_CATEGORIES, loadDraft, recentLogs, reportPlatform, saveDraft, techInfo } from './report'
  import { reportNav } from './report.svelte'
  import { messageOf, notify } from '../../lib/ui'

  let { onclose }: { onclose: () => void } = $props()

  interface Shot { blob: Blob; name: string; url: string; id?: string }
  const draft = loadDraft()
  let title = $state(draft.title)
  let description = $state(draft.description)
  let category = $state(draft.category)
  let shots = $state<Shot[]>([])
  const tech = techInfo()
  let included = $state<Record<string, boolean>>(Object.fromEntries(Object.keys(tech).map((k) => [k, true])))
  let withLogs = $state(false)
  let error = $state('')
  let busy = $state(false)
  let hidden = $state(false)

  $effect(() => saveDraft({ title, description, category }))

  const add = (blob: Blob, name: string) => {
    if (shots.length >= MAX_SHOTS) return notify.info(`${MAX_SHOTS} images au maximum.`)
    shots.push({ blob, name, url: URL.createObjectURL(blob) })
  }
  const addFiles = (files: Iterable<File>) => { for (const f of files) if (f.type.startsWith('image/')) add(f, f.name) }
  const remove = (i: number) => { URL.revokeObjectURL(shots[i].url); shots.splice(i, 1) }

  onMount(() => {
    if (reportNav.shot) add(reportNav.shot, 'capture.jpg')
    reportNav.shot = null
    return () => shots.forEach((s) => URL.revokeObjectURL(s.url))
  })

  // Navigateur : le formulaire disparaît le temps de la capture pour ne pas y figurer.
  async function capture() {
    hidden = true
    try {
      const b = await platform.screenshot!.capture()
      hidden = false
      if (b) add(b, 'capture.jpg')
    } catch {
      hidden = false
      notify.info('Capture annulée ou impossible.')
    }
  }

  const onpaste = (e: ClipboardEvent) => addFiles([...(e.clipboardData?.files ?? [])])
  const ondrop = (e: DragEvent) => { e.preventDefault(); addFiles([...(e.dataTransfer?.files ?? [])]) }

  async function submit(e: Event) {
    e.preventDefault()
    busy = true
    error = ''
    try {
      for (const s of shots) s.id ??= (await api.upload(s.blob, s.name)).id
      const info = Object.fromEntries(Object.entries(tech).filter(([k]) => included[k]))
      await api.sendReport({
        title: title.trim(), description: description.trim(), category,
        platform: included.Plateforme ? reportPlatform() : '', tech: info, logs: withLogs ? recentLogs() : '', file_ids: shots.map((s) => s.id!),
      })
      saveDraft({ title: '', description: '', category: '' })
      notify.ok('Signalement envoyé, merci !')
      onclose()
    } catch (err) {
      error = `${messageOf(err, 'Envoi impossible.')} Ton texte est conservé : tu peux réessayer.`
    } finally {
      busy = false
    }
  }
</script>

<svelte:window {onpaste} />
{#if !hidden}
  <Modal title="Signaler un bug" {onclose}>
    <form class="rp" onsubmit={submit} ondragover={(e) => e.preventDefault()} {ondrop}>
      <p class="rp-intro"><span class="rp-ic" aria-hidden="true"><Icon name="bug" size={20} /></span><span class="v-note">Ton brouillon est gardé si tu fermes.</span></p>
      {#if error}<div class="v-alert dgr" role="alert"><Icon name="alerte" size={18} /><p class="plbls-alert__corps">{error}</p></div>{/if}
      <div class="plbls-field">
        <label class="plbls-field__label" for="rp-title">Titre</label>
        <input id="rp-title" class="plbls-input" bind:value={title} maxlength="120" required />
      </div>
      <div class="plbls-field">
        <label class="plbls-field__label" for="rp-desc">Ce qui s'est passé et ce qui était attendu</label>
        <textarea id="rp-desc" class="plbls-input plbls-textarea" rows="5" bind:value={description} maxlength="4000" required></textarea>
      </div>
      <div class="plbls-field">
        <span class="plbls-field__label" id="rp-cat">Catégorie (facultatif)</span>
        <div class="chips" role="group" aria-labelledby="rp-cat">
          {#each REPORT_CATEGORIES as c (c.id)}<button type="button" class="v-chip" aria-pressed={category === c.id} onclick={() => (category = c.id)}>{c.label}</button>{/each}
        </div>
      </div>

      <fieldset class="rp-box">
        <legend class="v-lbl">Images · {shots.length}/{MAX_SHOTS}</legend>
        <div class="rp-shots">
          {#each shots as s, i (s.url)}
            <div class="rp-shot"><img src={s.url} alt={s.name} /><button type="button" class="v-ib sm" aria-label="Retirer {s.name}" onclick={() => remove(i)}><Icon name="x" size={14} /></button></div>
          {/each}
          {#if platform.screenshot && !platform.screenshot.auto}<button type="button" class="plbls-button plbls-button--compact plbls-button--contour rp-add" onclick={capture}><Icon name="ecran" size={16} />Capturer la fenêtre</button>{/if}
          <label class="plbls-button plbls-button--compact plbls-button--discret rp-add"><Icon name="plus" size={16} />Ajouter des images<input type="file" accept="image/*" multiple hidden onchange={(e) => { addFiles([...(e.currentTarget.files ?? [])]); e.currentTarget.value = '' }} /></label>
        </div>
        <p class="v-note">Tu peux aussi glisser-déposer ou coller des images.</p>
      </fieldset>

      <details class="rp-det">
        <summary class="v-lbl">Informations jointes · {Object.values(included).filter(Boolean).length}</summary>
        <div class="rp-det-b">
          {#each Object.entries(tech) as [k, v] (k)}
            <label class="plbls-checkbox rp-tech"><input class="plbls-checkbox__input" type="checkbox" bind:checked={included[k]} /><span class="plbls-checkbox__case" aria-hidden="true"></span><span class="plbls-checkbox__texte"><strong>{k}</strong> : <span class="v-mono">{v}</span></span></label>
          {/each}
        </div>
      </details>
      <label class="plbls-checkbox rp-tech">
        <input class="plbls-checkbox__input" type="checkbox" bind:checked={withLogs} disabled={!debug.enabled || !debug.entries.length} />
        <span class="plbls-checkbox__case" aria-hidden="true"></span>
        <span class="plbls-checkbox__texte v-note"><b>Joindre les logs</b> (dernières lignes du mode debug, sans jeton ni contenu de message){#if !debug.enabled} — activez le mode debug dans les paramètres, reproduisez le bug, puis signalez-le.{/if}</span>
      </label>

      <div class="rp-f">
        <button type="button" class="plbls-button plbls-button--contour" onclick={onclose}>Annuler</button>
        <button class="plbls-button" disabled={busy || !title.trim() || !description.trim()}>{busy ? 'Envoi…' : 'Envoyer'}</button>
      </div>
    </form>
  </Modal>
{/if}

<style>
  .rp { display: flex; flex-direction: column; gap: 14px; }
  .rp-intro { display: flex; align-items: center; gap: 10px; margin: -6px 0 0; }
  .rp-ic { display: grid; place-items: center; width: 36px; height: 36px; border-radius: 12px; background: var(--rose); color: var(--encre); flex: none; }
  .chips { display: flex; flex-wrap: wrap; gap: 6px; }
  .v-chip:focus-visible { outline: none; box-shadow: var(--anneau); }
  .rp-box { display: flex; flex-direction: column; gap: 8px; min-width: 0; margin: 0; padding: 0; border: 0; }
  .rp-box legend { padding: 0; margin-bottom: 8px; }
  .rp-box p { margin: 0; }
  .rp-shots { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; }
  .rp-shot { position: relative; }
  .rp-shot img { display: block; height: 54px; border-radius: var(--r-s); box-shadow: var(--o1); }
  .rp-shot button { position: absolute; top: -8px; right: -8px; background: var(--surface); color: var(--encre); box-shadow: var(--o2); border-radius: 50%; }
  .rp-add { height: 54px; }
  .rp-det { border-top: 1px solid var(--trait); padding-top: 10px; }
  .rp-det > summary { cursor: pointer; }
  .rp-det > summary:focus-visible { outline: none; box-shadow: var(--anneau); border-radius: var(--r-xs); }
  .rp-det-b { display: flex; flex-direction: column; gap: 4px; margin-top: 8px; }
  .rp-tech { display: flex; overflow-wrap: anywhere; }
  textarea { resize: vertical; }
  .rp-f { display: flex; justify-content: flex-end; gap: 8px; flex-wrap: wrap; }
  @media (pointer: coarse) { .v-chip { min-height: 44px; } .rp-shot button { width: 36px; height: 36px; } }
</style>
