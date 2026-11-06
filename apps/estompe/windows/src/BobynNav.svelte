<script lang="ts">
  import Icon from '../../../../web/estompe/src/components/Icon.svelte'
  import { hrefs, router } from '../../../../web/estompe/src/lib/route.svelte'

  const section = $derived(router.current.name === 'space' ? router.current.section : undefined)
</script>

<div class="side-head"><h2>Bobyn</h2></div>
<div class="side-list">
  <a class="nav-item" class:active={section !== 'settings'} href={hrefs.space('bobyn')}><Icon name="film" size={16} /><span class="truncate">Replays</span></a>
  <a class="nav-item" class:active={section === 'settings'} href={hrefs.space('bobyn', 'settings')}><Icon name="sliders" size={16} /><span class="truncate">Réglages</span></a>
</div>
