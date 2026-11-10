import { encryptPayload } from './crypto.js';
import { toast } from './vendor/plbls-ui/plbls-ui.esm.min.js';

const $ = (id) => document.getElementById(id);
let cfg, file = null;

const msg = (text, error) => {
  $('msg').className = text ? 'plbls-alert' + (error ? ' plbls-alert--erreur' : '') : '';
  $('msg').textContent = text || '';
  if (text && error) $('msg').setAttribute('role', 'alert'); else $('msg').setAttribute('role', 'status');
};
const fmtSize = (n) => n < 1024 ? `${n} o` : n < 1048576 ? `${(n / 1024).toFixed(1)} Ko` : `${(n / 1048576).toFixed(1)} Mo`;
const fmtDate = (d) => new Date(d).toLocaleString('fr-FR', { dateStyle: 'short', timeStyle: 'short' });
const fmtTtl = (s) => s < 86400 ? `${s / 3600} heure` : `${s / 86400} jour${s > 86400 ? 's' : ''}`;

async function api(method, path, body, headers) {
  const res = await fetch(path, { method, body, headers });
  const out = await res.json().catch(() => ({}));
  if (!res.ok) throw Object.assign(new Error(out.error || 'HTTP ' + res.status), { status: res.status, reason: out.reason, userId: out.user_id });
  return out;
}

function setFile(f) {
  file = f;
  $('file-row').hidden = !f;
  $('file-name').textContent = f ? f.name : '';
  $('file-size').textContent = f ? fmtSize(f.size) : '';
}

async function refreshList() {
  const rows = await api('GET', '/api/shares');
  $('none').hidden = rows.length > 0;
  $('shares').replaceChildren(...rows.map((r) => {
    const tr = document.createElement('tr');
    for (const t of [fmtDate(r.created_at), fmtSize(r.size), fmtDate(r.expires_at)]) { const td = document.createElement('td'); td.textContent = t; tr.append(td); }
    const act = document.createElement('td'), b = document.createElement('button');
    b.type = 'button'; b.className = 'plbls-button plbls-button--compact plbls-button--discret'; b.textContent = 'Révoquer';
    b.onclick = async () => { try { await api('DELETE', `/api/shares/${r.id}`); toast.success('Partage révoqué'); await refreshList(); } catch (e) { msg(e.message, true); } };
    act.append(b); tr.append(act); return tr;
  }));
}

async function submit(e) {
  e.preventDefault(); msg('');
  const isFile = !$('pane-file').hidden;
  let header, content;
  if (isFile) {
    if (!file) return msg('Choisissez un fichier.', true);
    header = { kind: 'file', name: file.name, type: file.type || '' };
    content = new Uint8Array(await file.arrayBuffer());
  } else {
    const t = $('text').value;
    if (!t.trim()) return msg('Écrivez un message.', true);
    header = { kind: 'text' };
    content = new TextEncoder().encode(t);
  }
  if (content.length + 100 > cfg.maxBytes) return msg(`Trop volumineux (maximum ${fmtSize(cfg.maxBytes - 100)}).`, true);
  $('send').disabled = true; $('send').classList.add('is-busy');
  try {
    const { blob, key } = await encryptPayload(header, content);
    const out = await api('POST', `/api/shares?ttl=${$('ttl').value}`, blob, { 'content-type': 'application/octet-stream' });
    $('link').setAttribute('value', `${location.origin}/s/${out.id}#${key}`);
    $('form').hidden = true; $('result').hidden = false;
    $('text').value = ''; setFile(null);
    await refreshList();
  } catch (err) {
    if (err.status === 401) location.reload(); else msg(err.message, true);
  } finally { $('send').disabled = false; $('send').classList.remove('is-busy'); }
}

async function start() {
  cfg = await api('GET', '/api/config');
  $('limit').textContent = `Taille maximale : ${fmtSize(cfg.maxBytes - 100)}`;
  $('ttl').replaceChildren(...cfg.ttls.map((s) => new Option(fmtTtl(s), s, false, s === 86400)));
  try {
    await api('GET', '/api/me');
  } catch (e) {
    if (e.status === 401) {
      $('login').hidden = false;
      $('login-link').href = `${cfg.gatewayUrl}/auth/login?redirect=${encodeURIComponent(location.href)}`;
    } else msg(e.status === 403
      ? `Accès réservé : il faut la permission ${cfg.permission} (aegis). Compte connecté : ${e.userId}${e.reason ? ' — ' + e.reason : ''}`
      : e.message, true);
    return;
  }
  $('logout').action = `${cfg.gatewayUrl}/auth/logout?redirect=${encodeURIComponent(location.origin)}`;
  $('logout').hidden = false; $('app').hidden = false;

  document.querySelector('plbls-tabs').addEventListener('plbls-change', (ev) => {
    const f = ev.detail === 't-file';
    $('pane-text').hidden = f; $('pane-file').hidden = !f;
  });
  document.querySelector('plbls-dropzone').addEventListener('plbls-files', (ev) => setFile(ev.detail[0]));
  $('file-clear').onclick = () => setFile(null);
  $('form').onsubmit = submit;
  $('again').onclick = () => { $('result').hidden = true; $('form').hidden = false; $('link').setAttribute('value', ''); };
  await refreshList();
}
start().catch((e) => msg(e.message, true));
