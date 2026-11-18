<script lang="ts">
  import { onMount } from 'svelte'
  import Icon from '../../components/Icon.svelte'
  import { router, hrefs } from '../../lib/route.svelte'
  import { notify } from '../../lib/ui'
  import { NOM_ACTION } from './admin'
  import { messageDe } from './admin.svelte'
  import { confirmerMonEffacement } from './effacement'
  import { dateHeure } from './formats'
  import { enregistrer, sablierApi, type Vue } from './sablier'
  import { sablier } from './sablier.svelte'

  let export_ = $state<'' | 'cours' | 'echec'>('')
  let erreurExport = $state('')
  let effacement = $state(false)
  let vues = $state.raw<Vue[]>([])

  onMount(() => { sablierApi.vues().then((v) => (vues = v), () => {}) })

  async function exporter() {
    if (export_ === 'cours') return
    export_ = 'cours'
    erreurExport = ''
    try {
      const f = await sablierApi.exporter()
      enregistrer(f.blob, f.nom)
      export_ = ''
    } catch (e) {
      erreurExport = messageDe(e)
      export_ = 'echec'
    }
  }

  async function effacer() {
    if (effacement || !(await confirmerMonEffacement())) return
    effacement = true
    try {
      await sablierApi.effacer()
      sablier.reinitialiser()
      void sablier.chargerNonVus()
      notify.ok('Tes stats sont effacées.')
      router.go(hrefs.stats())
    } catch (e) {
      notify.error(e, 'Tes stats n’ont pas pu être effacées')
    } finally {
      effacement = false
    }
  }
</script>

<section class="sab-sec" aria-label="Mes données">
  <h2 class="sab-h2">Mes données</h2>
  <div class="sab-g sab-g2">
    <div class="sab-card">
      <div class="sab-card-tete"><Icon name="telecharge" size={18} />Exporter</div>
      <p class="sab-note">Un fichier JSON avec tout ce que Sablier garde sur toi : évènements encore conservés, totaux par jour, trophées, récaps, réglages de partage et journal des consultations.</p>
      {#if export_ === 'echec'}<p class="sab-note sab-erreur" role="alert">L’export a échoué. {erreurExport}</p>{/if}
      <div class="sab-ligne"><button class="plbls-button plbls-button--contour" type="button" disabled={export_ === 'cours'} aria-busy={export_ === 'cours'} onclick={exporter}>{export_ === 'cours' ? 'Préparation du fichier…' : export_ === 'echec' ? 'Réessayer l’export' : 'Télécharger mes données'}</button></div>
    </div>
    <div class="sab-card">
      <div class="sab-card-tete"><Icon name="corbeille" size={18} />Tout effacer</div>
      <p class="sab-note">Efface définitivement tes stats, trophées, récaps et réglages de partage. Ton historique Entracte et Sillon et tes messages ne sont pas touchés, et le journal des consultations est conservé. La collecte continue : tes stats repartent de zéro.</p>
      <div class="sab-ligne"><button class="plbls-button plbls-button--danger" type="button" disabled={effacement} onclick={effacer}>Effacer mes stats…</button></div>
    </div>
  </div>
</section>

{#if vues.length}
  <section class="sab-sec" aria-label="Journal des consultations">
    <div class="sab-tete"><h2 class="sab-h2">Qui a consulté mes stats</h2><p class="sab-note">Chaque fois qu’un admin ouvre tes stats, c’est noté ici.</p></div>
    <ul class="sab-card sab-card--serre">
      {#each vues as v, n (n)}
        <li class="sab-reg sab-reg--fixe"><span class="sab-col sab-grow" style:gap="0"><b>{v.admin_name}</b><span class="sab-note">{NOM_ACTION[v.action]}</span></span><span class="sab-note">{dateHeure(v.date)}</span></li>
      {/each}
    </ul>
  </section>
{/if}
