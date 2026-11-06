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
      error = `${messageOf(err, 'Envoi impossible.')} Votre texte est conservé : vous pouvez réessayer.`
    } finally {
      busy = false
    }
  }
</script>

<svelte:window {onpaste} />
{#if !hidden}
  <Modal title="Signaler un bug" {onclose}>
    <form class="stack" onsubmit={submit} ondragover={(e) => e.preventDefault()} {ondrop}>
      {#if error}<div class="plbls-alert plbls-alert--erreur" role="alert"><p class="plbls-alert__corps">{error}</p></div>{/if}
      <div class="plbls-field">
        <label class="plbls-field__label" for="rp-title">Titre</label>
        <input id="rp-title" class="plbls-input" bind:value={title} maxlength="120" required />
      </div>
      <div class="plbls-field">
        <label class="plbls-field__label" for="rp-desc">Ce qui s'est passé et ce qui était attendu</label>
        <textarea id="rp-desc" class="plbls-input" rows="5" bind:value={description} maxlength="4000" required></textarea>
      </div>
      <div class="plbls-field">
        <label class="plbls-field__label" for="rp-cat">Catégorie (facultatif)</label>
        <select id="rp-cat" class="plbls-select" bind:value={category}>{#each REPORT_CATEGORIES as c (c.id)}<option value={c.id}>{c.label}</option>{/each}</select>
      </div>

      <fieldset class="rp-box">
        <legend>Images ({shots.length}/{MAX_SHOTS})</legend>
        <div class="rp-shots">
          {#each shots as s, i (s.url)}
            <div class="rp-shot"><img src={s.url} alt={s.name} /><button type="button" class="ib sm" aria-label="Retirer {s.name}" onclick={() => remove(i)}><Icon name="x" size={14} /></button></div>
          {/each}
        </div>
        <div class="row">
          {#if platform.screenshot && !platform.screenshot.auto}<button type="button" class="plbls-button plbls-button--compact plbls-button--contour" onclick={capture}>Capturer la fenêtre</button>{/if}
          <label class="plbls-button plbls-button--compact plbls-button--contour">Ajouter des images<input type="file" accept="image/*" multiple hidden onchange={(e) => { addFiles([...(e.currentTarget.files ?? [])]); e.currentTarget.value = '' }} /></label>
        </div>
        <p class="muted small">Vous pouvez aussi glisser-déposer ou coller des images.</p>
      </fieldset>

      <fieldset class="rp-box">
        <legend>Informations jointes</legend>
        {#each Object.entries(tech) as [k, v] (k)}
          <label class="plbls-checkbox rp-tech"><input class="plbls-checkbox__input" type="checkbox" bind:checked={included[k]} /><span class="plbls-checkbox__case" aria-hidden="true"></span><span class="plbls-checkbox__texte"><strong>{k}</strong> : {v}</span></label>
        {/each}
        <label class="plbls-checkbox rp-tech">
          <input class="plbls-checkbox__input" type="checkbox" bind:checked={withLogs} disabled={!debug.enabled || !debug.entries.length} />
          <span class="plbls-checkbox__case" aria-hidden="true"></span>
          <span class="plbls-checkbox__texte"><strong>Joindre les logs</strong> (dernières lignes du mode debug, sans jeton ni contenu de message){#if !debug.enabled} — activez le mode debug dans les paramètres, reproduisez le bug, puis signalez-le.{/if}</span>
        </label>
      </fieldset>

      <button class="plbls-button" disabled={busy || !title.trim() || !description.trim()}>{busy ? 'Envoi…' : 'Envoyer'}</button>
    </form>
  </Modal>
{/if}

<style>
  .rp-box { border: 1px solid var(--border, currentColor); border-radius: 0.5rem; padding: 0.5rem 0.75rem; min-width: 0 }
  .rp-tech { display: flex; margin: 0.25rem 0; overflow-wrap: anywhere }
  .rp-shots { display: flex; flex-wrap: wrap; gap: 0.5rem; margin-bottom: 0.5rem }
  .rp-shot { position: relative }
  .rp-shot img { height: 5rem; border-radius: 0.375rem; display: block }
  .rp-shot button { position: absolute; top: 0.125rem; right: 0.125rem }
  textarea { resize: vertical }
</style>
