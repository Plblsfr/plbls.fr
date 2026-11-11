<script lang="ts">
  import Icon from '../components/Icon.svelte'
  import { loginUrl, registerUrl } from '../lib/auth'
  import { app } from '../lib/store.svelte'
  import { platform } from '../lib/platform'
  import { messageOf, notify } from '../lib/ui'

  const auth = platform.auth
  const Downloads = platform.slots.login
  let busy = $state(false)

  async function signIn(register: boolean) {
    busy = true
    try {
      await auth!.login(register)
      await app.boot()
      if (app.phase === 'login') notify.fail(`Connexion refusée par le serveur : ${await auth!.diagnose()}`)
    } catch (e) {
      // Tauri rejette avec une simple chaîne : on l'affiche telle quelle plutôt que le message générique
      notify.fail(typeof e === 'string' && e ? e : e instanceof Error && e.message ? e.message : messageOf(e, 'Connexion impossible'))
    } finally {
      busy = false
    }
  }

  const atouts: { icon: 'bulle' | 'vol' | 'film'; titre: string; texte: string }[] = [
    { icon: 'bulle', titre: 'Salons et MP', texte: 'Sondages, fichiers, messages vocaux' },
    { icon: 'vol', titre: 'Vocal', texte: 'Caméra et partage d\'écran' },
    { icon: 'film', titre: 'Ensemble', texte: 'Vidéos, musique, dessin' },
  ]
</script>

<div class="connexion">
  <section class="vitrine" aria-labelledby="connexion-titre">
    <div class="marque"><span class="acces-logo" aria-hidden="true">E</span><span class="nom">Estompe</span></div>
    <div class="accroche">
      <h1 id="connexion-titre">Écrire, parler,<br />regarder <span class="v-surl">ensemble</span>.</h1>
      <p>Messagerie écrite et vocale pour un petit groupe d'amis.</p>
    </div>
    <ul class="atouts">
      {#each atouts as a (a.titre)}
        <li><span class="atout-icone"><Icon name={a.icon} size={22} /></span><b>{a.titre}</b><span class="v-note">{a.texte}</span></li>
      {/each}
    </ul>
  </section>

  <section class="entree" aria-labelledby="entree-titre">
    <div class="pile">
      <div class="tete">
        <h2 id="entree-titre">Bon retour.</h2>
        <p class="v-mute">Ta connexion passe par le compte plbls.fr.</p>
      </div>
      <div class="actions">
        {#if auth}
          <button class="plbls-button plbls-button--block" class:is-busy={busy} disabled={busy} onclick={() => signIn(false)}>Se connecter</button>
          <button class="plbls-button plbls-button--contour plbls-button--block" disabled={busy} onclick={() => signIn(true)}>Créer un compte</button>
        {:else}
          <a class="plbls-button plbls-button--block" href={loginUrl()}>Se connecter</a>
          <a class="plbls-button plbls-button--contour plbls-button--block" href={registerUrl()}>Créer un compte</a>
        {/if}
      </div>
      {#if busy}
        <div class="v-well attente" role="status">
          <span class="v-wave" aria-hidden="true"><i style="height: 8px"></i><i style="height: 16px"></i><i style="height: 11px"></i></span>
          <span class="attente-texte"><b>En attente du navigateur</b><span class="v-note">Termine la connexion dans ton navigateur, puis reviens ici.</span></span>
        </div>
      {/if}
      {#if Downloads}
        <div class="applis">
          <span class="v-eyebrow">Applications</span>
          <Downloads />
        </div>
      {/if}
      <hr class="v-hr" />
      <p class="v-note">L'accès à Estompe est accordé par l'administrateur.</p>
    </div>
  </section>
</div>

<style>
  .connexion { height: 100%; display: flex; flex-wrap: wrap; background: var(--papier); overflow: auto; }
  .vitrine { flex: 1 1 560px; min-width: 0; display: flex; flex-direction: column; justify-content: space-between; gap: 40px; padding: 56px; background: var(--papier-2); border-right: 1px solid var(--trait); }
  .marque { display: flex; align-items: center; gap: 12px; }
  .nom { font-family: var(--f-titre); font-size: 22px; font-weight: 700; letter-spacing: -0.03em; }
  .accroche { display: flex; flex-direction: column; gap: 24px; }
  .accroche h1 { margin: 0; font-family: var(--f-titre); font-size: clamp(44px, 7vw, 88px); font-weight: 700; line-height: 0.95; letter-spacing: -0.05em; }
  .accroche p { margin: 0; max-width: 520px; color: var(--encre-2); font-size: 19px; line-height: 1.45; }
  .atouts { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 12px; max-width: 640px; margin: 0; padding: 0; list-style: none; }
  .atouts li { display: flex; flex-direction: column; gap: 6px; padding: 16px; border-radius: var(--r-l); background: var(--surface); box-shadow: var(--o1); }
  .atout-icone { color: var(--rose-texte); }
  .entree { flex: 1 1 440px; min-width: 0; display: flex; align-items: center; justify-content: center; padding: 56px 32px; }
  .pile { width: 100%; max-width: 400px; display: flex; flex-direction: column; gap: 24px; }
  .tete { display: flex; flex-direction: column; gap: 8px; }
  .tete h2 { margin: 0; font-family: var(--f-titre); font-size: 36px; font-weight: 700; letter-spacing: -0.03em; line-height: 1.1; }
  .tete p, .pile > p { margin: 0; }
  .actions { display: flex; flex-direction: column; gap: 10px; }
  .attente { display: flex; align-items: flex-start; gap: 12px; }
  .attente .v-wave { height: 18px; padding-top: 2px; }
  .attente-texte { display: flex; flex-direction: column; gap: 4px; font-size: 14px; }
  .applis { display: flex; flex-direction: column; gap: 10px; }
  @media (max-width: 900px) {
    .connexion { flex-direction: column; flex-wrap: nowrap; }
    .vitrine { flex: none; padding: 32px 16px 0; border: 0; background: none; }
    .accroche, .atouts { display: none; }
    .entree { flex: 1 0 auto; padding: 24px 16px 40px; }
  }
  @media (pointer: coarse) {
    .actions :global(.plbls-button) { min-height: 44px; }
  }
</style>
