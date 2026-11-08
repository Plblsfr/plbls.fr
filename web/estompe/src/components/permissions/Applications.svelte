<script lang="ts">
  import { confirmer } from '../../lib/confirm.svelte'
  import { api } from '../../lib/api'
  import { charger, enc, executer, type AppInfo } from '../../lib/permissions.svelte'
  import Application from './Application.svelte'
  import Attente from './Attente.svelte'

  const apps = charger<AppInfo[]>(() => '/apps')
  let sel = $state<string | null>(null)
  let key = $state('')
  let name = $state('')
  const dirty = { current: false }
  const guard = async () => !dirty.current || confirmer({ titre: 'Modifications non enregistrées', message: 'Abandonner les modifications non enregistrées de ce rôle ?', action: 'Abandonner', destructif: true })
  const selected = $derived(sel ? apps.data?.find((a) => a.key === sel) : undefined)

  const creer = () => {
    const k = key
    void executer(api.permissions('POST', '/apps', { key: k, name }), `Application ${k} créée`).then((ok) => { if (ok) { sel = k; key = ''; name = ''; apps.reload() } })
  }
  const supprimer = async (k: string) => {
    if (!(await confirmer({ titre: "Supprimer l'application", message: `Supprimer l'application ${k}, ses permissions, ses rôles et toutes les attributions associées ?`, action: 'Supprimer', destructif: true, saisie: { label: "Saisissez la clé de l'application pour confirmer", exiger: k } }))) return
    void executer(api.permissions('DELETE', `/apps/${enc(k)}`), `Application ${k} supprimée`).then((ok) => { if (ok) { sel = null; apps.reload() } })
  }
</script>

<div class="perm-cols">
  <section class="v-card perm-carte">
    <h2 class="perm-t">Applications</h2>
    {#if apps.data === null}
      <Attente error={apps.error} retry={apps.reload} />
    {:else if apps.data.length === 0}
      <p class="v-note">Aucune application. Crée la première ci-dessous.</p>
    {:else}
      <div class="perm-lignes">
        {#each apps.data as a (a.key)}
          <button class="perm-ligne" aria-current={a.key === sel} onclick={async () => { if (a.key !== sel && (await guard())) sel = a.key }}><b>{a.name}</b><span class="v-mono v-mute">{a.key}</span></button>
        {/each}
      </div>
    {/if}
    <details class="perm-nouveau" open={apps.data?.length === 0}>
      <summary>Nouvelle application</summary>
      <form class="perm-pile" onsubmit={(e) => { e.preventDefault(); creer() }}>
        <div class="plbls-field">
          <label class="plbls-field__label" for="perm-app-key">Clé</label>
          <input class="plbls-input" id="perm-app-key" bind:value={key} placeholder="chat" pattern={'[a-z][a-z0-9\\-]{0,39}'} required aria-describedby="perm-app-key-aide" />
          <span class="plbls-field__aide" id="perm-app-key-aide">minuscules, chiffres et tirets</span>
        </div>
        <div class="plbls-field">
          <label class="plbls-field__label" for="perm-app-name">Nom</label>
          <input class="plbls-input" id="perm-app-name" bind:value={name} placeholder="Chat" required />
        </div>
        <button class="plbls-button plbls-button--block">Créer l'application</button>
      </form>
    </details>
  </section>

  {#if selected}
    {#key selected.key}
      <Application app={selected} onDelete={() => supprimer(selected.key)} {dirty} {guard} />
    {/key}
  {:else}
    <div class="v-empty perm-vide">
      <b>Choisis une application</b>
      <p class="v-note">Ses permissions et ses rôles s'affichent ici.</p>
    </div>
  {/if}
</div>
