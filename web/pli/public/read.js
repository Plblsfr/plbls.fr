import { decryptPayload, validKey, safeName } from './crypto.js';
import { toast } from './vendor/plbls-ui/plbls-ui.esm.min.js';

const $ = (id) => document.getElementById(id);
const id = location.pathname.split('/').pop();
const key = location.hash.slice(1);
const fmtSize = (n) => n < 1024 ? `${n} o` : n < 1048576 ? `${(n / 1024).toFixed(1)} Ko` : `${(n / 1048576).toFixed(1)} Mo`;

function gone(title = 'Ce message n\'existe plus', text = 'Il a déjà été lu, il a expiré, ou le lien est incorrect.') {
  $('ready').hidden = true; $('shown').hidden = true; $('gone').hidden = false;
  $('gone-title').textContent = title; $('gone-text').textContent = text;
}

async function open() {
  $('open').disabled = true;
  let blob;
  try {
    const res = await fetch(`/api/s/${id}/consume`, { method: 'POST' });
    if (res.status === 429) { $('open').disabled = false; return toast.error('Trop de tentatives, réessayez dans une minute.'); }
    if (!res.ok) return gone();
    blob = new Uint8Array(await res.arrayBuffer());
  } catch { $('open').disabled = false; return toast.error('Réseau indisponible. Le message n\'a pas été ouvert, réessayez.'); }
  // Le message est déjà effacé côté serveur : en cas d'échec ici, il est perdu (clé fausse ou contenu altéré).
  let out;
  try { out = await decryptPayload(blob, key); } catch (e) {
    return gone('Impossible de déchiffrer', `${e.message}. Le message a été effacé du serveur : demandez à l'expéditeur d'en créer un nouveau.`);
  }
  history.replaceState(null, '', location.pathname); // la clé quitte la barre d'adresse et l'historique
  $('ready').hidden = true; $('shown').hidden = false;
  if (out.header.kind === 'file') {
    const name = safeName(out.header.name);
    const url = URL.createObjectURL(new Blob([out.content], { type: 'application/octet-stream' }));
    $('file').hidden = false; $('file-name').textContent = name; $('file-size').textContent = fmtSize(out.content.length);
    $('download').hidden = false; $('download').href = url; $('download').download = name;
  } else {
    const text = new TextDecoder().decode(out.content);
    $('text').hidden = false; $('text').textContent = text;
    $('copy').hidden = false;
    $('copy').onclick = async () => { try { await navigator.clipboard.writeText(text); toast.success('Copié'); } catch { toast.error('Copie impossible : sélectionnez le texte.'); } };
  }
}

async function start() {
  if (!validKey(key)) return gone('Lien incomplet', 'La clé de déchiffrement manque dans le lien. Copiez-le en entier, avec la partie après le « # ». Le message n\'a pas été ouvert.');
  const res = await fetch(`/api/s/${id}`);
  if (res.status === 429) return gone('Trop de requêtes', 'Réessayez dans une minute.');
  if (!res.ok) return gone();
  const meta = await res.json();
  $('meta').textContent = `Taille : ${fmtSize(meta.size)} · expire le ${new Date(meta.expires_at).toLocaleString('fr-FR', { dateStyle: 'long', timeStyle: 'short' })}`;
  $('ready').hidden = false;
  $('open').onclick = open;
}
start().catch(() => gone('Erreur', 'Impossible de joindre le serveur.'));
