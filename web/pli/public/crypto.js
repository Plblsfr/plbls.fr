// Chiffrement de bout en bout (WebCrypto, AES-256-GCM). La clé ne quitte jamais le navigateur :
// elle voyage dans le fragment (#…) du lien, que le navigateur n'envoie pas au serveur.
//
// Blob envoyé au serveur : 0x01 | IV (12 octets) | AES-GCM( 4 octets = longueur de l'en-tête | en-tête JSON | contenu ).
// Le nom, le type et la nature (texte/fichier) sont dans la partie chiffrée : le serveur ne voit que la taille.
const VERSION = 1;
const AAD = new TextEncoder().encode('pli/1');
const b64u = {
  enc: (u8) => btoa(String.fromCharCode(...u8)).replaceAll('+', '-').replaceAll('/', '_').replaceAll('=', ''),
  dec: (s) => Uint8Array.from(atob(s.replaceAll('-', '+').replaceAll('_', '/')), (c) => c.charCodeAt(0)),
};

export const KEY_LENGTH = 43; // 32 octets en base64url sans bourrage

export async function encryptPayload(header, content) {
  const key = await crypto.subtle.generateKey({ name: 'AES-GCM', length: 256 }, true, ['encrypt']);
  const head = new TextEncoder().encode(JSON.stringify(header));
  const plain = new Uint8Array(4 + head.length + content.length);
  new DataView(plain.buffer).setUint32(0, head.length);
  plain.set(head, 4); plain.set(content, 4 + head.length);
  const iv = crypto.getRandomValues(new Uint8Array(12));
  const sealed = new Uint8Array(await crypto.subtle.encrypt({ name: 'AES-GCM', iv, additionalData: AAD }, key, plain));
  const blob = new Uint8Array(1 + 12 + sealed.length);
  blob[0] = VERSION; blob.set(iv, 1); blob.set(sealed, 13);
  return { blob, key: b64u.enc(new Uint8Array(await crypto.subtle.exportKey('raw', key))) };
}

export const validKey = (k) => typeof k === 'string' && k.length === KEY_LENGTH && /^[A-Za-z0-9_-]+$/.test(k);

export async function decryptPayload(blob, keyB64) {
  if (!validKey(keyB64)) throw new Error('clé invalide');
  if (blob[0] !== VERSION) throw new Error('format inconnu');
  const key = await crypto.subtle.importKey('raw', b64u.dec(keyB64), 'AES-GCM', false, ['decrypt']);
  let plain;
  try {
    plain = new Uint8Array(await crypto.subtle.decrypt(
      { name: 'AES-GCM', iv: blob.subarray(1, 13), additionalData: AAD }, key, blob.subarray(13)));
  } catch { throw new Error('déchiffrement impossible : clé incorrecte ou contenu altéré'); }
  const n = new DataView(plain.buffer).getUint32(0);
  if (4 + n > plain.length) throw new Error('contenu corrompu');
  const header = JSON.parse(new TextDecoder().decode(plain.subarray(4, 4 + n)));
  return { header, content: plain.subarray(4 + n) };
}

// Nom de fichier sûr pour le téléchargement (pas de chemin, pas de caractères de contrôle).
export function safeName(name) {
  const n = String(name || '').replace(/[\\/\u0000-\u001f\u007f]/g, '_').replace(/^\.+/, '').trim().slice(0, 150);
  return n || 'fichier';
}
