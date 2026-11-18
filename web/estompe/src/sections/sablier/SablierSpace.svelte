<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import { router, hrefs } from '../../lib/route.svelte'
  import { app } from '../../lib/store.svelte'
  import SablierAdmin from './SablierAdmin.svelte'
  import SablierJeu from './SablierJeu.svelte'
  import SablierPartage from './SablierPartage.svelte'
  import SablierRecap from './SablierRecap.svelte'
  import SablierRecaps from './SablierRecaps.svelte'
  import { sablier } from './sablier.svelte'

  let { section, id, detail, onmenu }: { section?: string; id?: string; detail?: string; onmenu: () => void } = $props()
  const recap = $derived(section === 'recaps' && id && /^\d+$/.test(id) ? Number(id) : null)
  const nouveau = $derived(sablier.nonVus.recaps > 0 && section !== 'recaps')
</script>

<header class="main-head film-head">
  <button class="ib burger sab-ancre" onclick={onmenu} aria-label={nouveau ? 'Menu, nouveau récap' : 'Menu'}><Icon name="menu" />{#if nouveau}<span class="sab-pip sab-pip--coin"></span>{/if}</button>
  <h1>Stats</h1>
</header>
{#if section === 'recaps'}
  <SablierRecaps />
  {#if recap !== null}{#key recap}<SablierRecap id={recap} onclose={() => router.go(hrefs.stats('recaps'))} />{/key}{/if}
{:else if section === 'partage'}
  <SablierPartage />
{:else if section === 'admin' && app.sablierMe?.permissions.admin}
  <SablierAdmin onglet={id} {detail} />
{:else}
  <SablierJeu />
{/if}
