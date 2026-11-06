<script lang="ts">
  import { onMount } from 'svelte'
  import { initials } from '../../frontend/src/lib/format'
  import { MSG_TOAST_SIZE, type MsgOverlayState } from './overlay'

  let st = $state<MsgOverlayState>({ toasts: [], replying: false })
  let text = $state('')
  let broken = $state<Record<string, boolean>>({})
  let field = $state<HTMLInputElement>()

  onMount(() => {
    let off = () => {}
    void import('@tauri-apps/api/event').then(async ({ listen, emit }) => {
      off = await listen<MsgOverlayState>('msg-overlay-state', (e) => { st = e.payload; broken = {}; if (!e.payload.replying) text = '' })
      await emit('msg-overlay-ready')
    })
    return () => off()
  })
  $effect(() => { if (st.replying) field?.focus() })

  async function key(e: KeyboardEvent) {
    if (e.key !== 'Enter' && e.key !== 'Escape') return
    e.preventDefault()
    const { emit } = await import('@tauri-apps/api/event')
    await emit(e.key === 'Enter' ? 'msg-overlay-send' : 'msg-overlay-cancel', text)
  }
</script>

<svelte:head>
  <style>html, body { background: transparent !important; overflow: hidden; margin: 0; }</style>
</svelte:head>

<div class="mo" style:--row="{MSG_TOAST_SIZE.row}px">
  {#each st.toasts as t (t.id)}
    <div class="mo-row">
      <span class="mo-av">
        {#if t.avatar && !broken[t.id]}<img src={t.avatar} alt="" onerror={() => (broken[t.id] = true)} />{:else}{initials(t.title)}{/if}
      </span>
      <span class="mo-txt"><b>{t.title}</b>{#if t.body}<span>{t.body}</span>{/if}</span>
    </div>
  {/each}
  {#if st.replying}
    <input class="mo-in" bind:this={field} bind:value={text} onkeydown={key} placeholder="Répondre… (Entrée : envoyer, Échap : annuler)" maxlength="2000" />
  {/if}
</div>

<style>
  .mo { display: flex; flex-direction: column; gap: 8px; font-family: system-ui, sans-serif; user-select: none; }
  .mo-row { height: var(--row); box-sizing: border-box; display: flex; align-items: center; gap: 10px; padding: 8px 10px; background: rgba(0, 0, 0, 0.75); border-radius: 10px; color: #fff; }
  .mo-av { width: 40px; height: 40px; flex: none; border-radius: 50%; overflow: hidden; display: grid; place-items: center; background: #444; font-weight: 600; }
  .mo-av img { width: 100%; height: 100%; object-fit: cover; }
  .mo-txt { display: flex; flex-direction: column; min-width: 0; font-size: 13px; }
  .mo-txt b, .mo-txt span { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .mo-txt span { opacity: 0.85; }
  .mo-in { height: 36px; box-sizing: border-box; padding: 0 10px; border-radius: 8px; border: 1px solid #888; background: rgba(0, 0, 0, 0.85); color: #fff; font-size: 14px; outline: none; }
</style>
