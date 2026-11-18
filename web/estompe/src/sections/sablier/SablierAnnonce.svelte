<script lang="ts">
  import Modal from '../../components/Modal.svelte'
  import { router, hrefs } from '../../lib/route.svelte'
  import { notify } from '../../lib/ui'
  import { deMois, jourLong, valeur } from './formats'
  import SablierMedaille from './SablierMedaille.svelte'
  import { sablier } from './sablier.svelte'
  import { estExploit, textePalier } from './stats'

  let vus = $state(0)
  let envoi = $state(false)
  const trophee = $derived(sablier.annonces[0])
  const recap = $derived(trophee ? null : sablier.recapAnnonce)
  const total = $derived(vus + sablier.annonces.length)

  function suivant() {
    if (!trophee) return
    vus++
    void sablier.tropheeVu(trophee.id)
  }

  async function afficher() {
    if (!trophee || envoi) return
    envoi = true
    const { id, name } = trophee
    try {
      await sablier.partager({ trophies: { [id]: true } })
      sablier.annonces = sablier.annonces.map((t) => (t.id === id ? { ...t, shared: true } : t))
      notify.ok(`« ${name} » est affiché sur ton profil.`)
    } catch (e) {
      notify.error(e, 'Le trophée n’a pas pu être affiché sur ton profil')
    } finally {
      envoi = false
    }
  }

  function regarder() {
    if (!recap) return
    const { id } = recap
    sablier.remettreRecap()
    router.go(hrefs.stats('recaps', id))
  }
</script>

{#if trophee}
  {#key trophee.id}
    <Modal title="Trophée débloqué" onclose={suivant}>
      <div class="sab-annonce" data-annonce="trophee">
        {#if total > 1}<p class="sab-note">{vus + 1} sur {total}</p>{/if}
        <SablierMedaille icone={trophee.icon} taille="xl" pose />
        <h3 class="sab-h1"><span class="sab-hl sab-hl--pose">{trophee.name}</span></h3>
        {#if !estExploit(trophee)}<div class="sab-pal" role="img" aria-label={textePalier(trophee)}>{#each { length: trophee.tiers } as _, i (i)}<i class:on={i < trophee.tier}></i>{/each}</div>{/if}
        <p>{trophee.description}{#if trophee.progress} Prochain palier à {valeur(trophee.progress.next_threshold, trophee.progress.unit)}.{/if}</p>
        <p class="sab-note">Débloqué le {jourLong(trophee.unlocked_at)}. {trophee.shared ? 'Affiché sur ton profil.' : 'Privé tant que tu ne l’affiches pas sur ton profil.'}</p>
        <div class="sab-annonce-actions">
          {#if !trophee.shared}<button class="plbls-button plbls-button--contour" type="button" disabled={envoi} onclick={afficher}>L’afficher sur mon profil</button>{/if}
          <button class="plbls-button" type="button" onclick={suivant}>{sablier.annonces.length > 1 ? 'Suivant' : 'Fermer'}</button>
        </div>
      </div>
    </Modal>
  {/key}
{:else if recap}
  <Modal title="Nouveau récap" onclose={() => sablier.remettreRecap()}>
    <div class="sab-annonce" data-annonce="recap">
      <SablierMedaille icone="histoire" taille="xl" pose />
      {#if recap.kind === 'year'}
        <h3 class="sab-h1">Ton année <span class="sab-hl sab-hl--pose">{recap.period.slice(0, 4)}</span> est prête</h3>
        <p>Quelques écrans pour revoir ton année : ce que tu as regardé, écouté et écrit.</p>
      {:else}
        {@const de = deMois(`${recap.period}-01`)}
        <h3 class="sab-h1">Ton mois {de.startsWith('d’') ? 'd’' : 'de '}<span class="sab-hl sab-hl--pose">{de.slice(de.startsWith('d’') ? 2 : 3)}</span> est prêt</h3>
        <p>Quelques écrans pour revoir ton mois : ce que tu as regardé, écouté et écrit.</p>
      {/if}
      <p class="sab-note">Il reste dans « Récaps », tu peux le regarder plus tard.</p>
      <div class="sab-annonce-actions">
        <button class="plbls-button plbls-button--contour" type="button" onclick={() => sablier.remettreRecap()}>Plus tard</button>
        <button class="plbls-button" type="button" onclick={regarder}>Le regarder</button>
      </div>
    </div>
  </Modal>
{/if}
