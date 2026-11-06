//! Buffer circulaire de paquets compressés, évincé par GOP entier.
use std::collections::VecDeque;
use std::sync::Arc;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Codec {
    H264,
    Hevc,
    Av1,
}

impl Codec {
    pub const ALL: [Codec; 3] = [Codec::H264, Codec::Hevc, Codec::Av1];

    pub fn label(self) -> &'static str {
        match self {
            Codec::H264 => "h264",
            Codec::Hevc => "hevc",
            Codec::Av1 => "av1",
        }
    }

    pub fn parse(s: &str) -> Option<Codec> {
        Self::ALL.into_iter().find(|c| c.label() == s)
    }
}

#[derive(Clone)]
pub struct Packet {
    pub pts_us: i64,
    pub keyframe: bool,
    pub data: Arc<[u8]>,
}

pub struct Ring {
    span_us: i64,
    // shortcut: un Vec par GOP (1 allocation/s) au lieu de blocs préalloués, à revoir si le profileur l'exige.
    gops: VecDeque<Vec<Packet>>,
}

impl Ring {
    pub fn new(span_us: i64) -> Self {
        Self { span_us, gops: VecDeque::new() }
    }

    pub fn push(&mut self, p: Packet) {
        if p.keyframe {
            self.gops.push_back(Vec::new());
        }
        // Sans image clé de départ, le paquet est inutilisable.
        let Some(gop) = self.gops.back_mut() else { return };
        let newest = p.pts_us;
        gop.push(p);
        while self.gops.len() > 1 && newest - self.gops[1][0].pts_us >= self.span_us {
            self.gops.pop_front();
        }
    }

    /// Copie (références partagées) de tout le buffer, commençant sur une image clé.
    pub fn snapshot(&self) -> Vec<Packet> {
        self.gops.iter().flatten().cloned().collect()
    }

    /// Les `span_us` dernières microsecondes, élargies à l'image clé qui précède (sans elle le début ne se décode pas).
    pub fn snapshot_last(&self, span_us: i64) -> Vec<Packet> {
        let Some(newest) = self.gops.back().and_then(|g| g.last()).map(|p| p.pts_us) else { return Vec::new() };
        let first = self.gops.iter().rposition(|g| g[0].pts_us <= newest - span_us).unwrap_or(0);
        self.gops.iter().skip(first).flatten().cloned().collect()
    }

    pub fn bytes(&self) -> usize {
        self.gops.iter().flatten().map(|p| p.data.len()).sum()
    }

    /// Heure de la plus ancienne image conservée.
    pub fn oldest_pts(&self) -> Option<i64> {
        self.gops.front().map(|g| g[0].pts_us)
    }
}

/// Cadence fixe de la vidéo : les pts sont ceux de la grille, pas l'heure (gigueuse) du réveil, sinon les durées d'images du MP4 sont irrégulières.
pub struct Pacer {
    interval: i64,
    tick: i64,
}

impl Pacer {
    pub fn new(interval: i64, start: i64) -> Self {
        Self { interval, tick: start }
    }

    pub fn due(&self) -> i64 {
        self.tick
    }

    /// Image émise à `now` (>= `due`) : renvoie son pts. Un retard de plus d'un intervalle resynchronise la grille sur `now` au lieu de rattraper en rafale.
    pub fn emit(&mut self, now: i64) -> i64 {
        let pts = if now - self.tick > self.interval { now } else { self.tick };
        self.tick = pts + self.interval;
        pts
    }
}

/// Format unique de tout l'audio : PCM 16 bits entrelacé, 48 kHz, stéréo (les sources sont converties à la capture).
pub const RATE: i64 = 48_000;
pub const CHANNELS: usize = 2;

/// Tampon circulaire de PCM brut ; chaque morceau garde l'heure QPC de sa première image.
pub struct PcmRing {
    span_us: i64,
    chunks: VecDeque<(i64, Box<[i16]>)>,
}

impl PcmRing {
    pub fn new(span_us: i64) -> Self {
        Self { span_us, chunks: VecDeque::new() }
    }

    pub fn push(&mut self, pts_us: i64, pcm: &[i16]) {
        self.chunks.push_back((pts_us, pcm.into()));
        while self.chunks.len() > 1 && pts_us - self.chunks[1].0 >= self.span_us {
            self.chunks.pop_front();
        }
    }

    pub fn bytes(&self) -> usize {
        self.chunks.iter().map(|c| c.1.len() * 2).sum()
    }

    /// Amplitude maximale du contenu (0 = silence complet).
    pub fn peak(&self) -> u16 {
        self.chunks.iter().flat_map(|c| c.1.iter()).map(|x| x.unsigned_abs()).max().unwrap_or(0)
    }

    /// Chronologie continue de `start_us` à `end_us` : les trous (la boucle système n'émet rien en silence)
    /// sont remplis de silence ; un écart de moins de 20 ms entre deux morceaux est de la gigue, pas un trou.
    pub fn render(&self, start_us: i64, end_us: i64) -> Vec<i16> {
        let frames = ((end_us - start_us).max(0) * RATE / 1_000_000) as usize;
        let mut out = vec![0i16; frames * CHANNELS];
        let mut next: Option<i64> = None; // position attendue du prochain morceau, en images
        for (pts, pcm) in &self.chunks {
            let at = (pts - start_us) * RATE / 1_000_000;
            let pos = match next {
                Some(n) if (at - n).abs() < RATE / 50 => n,
                _ => at,
            };
            next = Some(pos + (pcm.len() / CHANNELS) as i64);
            // Morceau partiellement avant le début : on saute la partie antérieure.
            let skip = (-pos).max(0) as usize;
            if skip * CHANNELS >= pcm.len() || pos >= frames as i64 {
                continue;
            }
            let dst = (pos.max(0) as usize) * CHANNELS;
            let n = (pcm.len() - skip * CHANNELS).min(out.len() - dst);
            out[dst..dst + n].copy_from_slice(&pcm[skip * CHANNELS..skip * CHANNELS + n]);
        }
        out
    }
}

/// Somme de pistes PCM de même longueur, écrêtée.
pub fn mix(tracks: &[&[i16]]) -> Vec<i16> {
    let mut sum = vec![0i32; tracks.first().map_or(0, |t| t.len())];
    for t in tracks {
        sum.iter_mut().zip(*t).for_each(|(m, s)| *m += *s as i32);
    }
    sum.into_iter().map(|m| m.clamp(i16::MIN as i32, i16::MAX as i32) as i16).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mix_somme_et_ecrete() {
        assert_eq!(mix(&[&[1, 30000, -5], &[2, 30000, -5], &[3, 0, 0]]), vec![6, 32767, -10]);
        assert!(mix(&[]).is_empty());
    }

    fn pkt(pts_s: i64, key: bool) -> Packet {
        Packet { pts_us: pts_s * 1_000_000, keyframe: key, data: Arc::from(vec![0u8; 10]) }
    }

    #[test]
    fn drops_until_first_keyframe() {
        let mut r = Ring::new(10_000_000);
        r.push(pkt(0, false));
        assert!(r.snapshot().is_empty());
    }

    #[test]
    fn last_seconds_start_on_a_keyframe() {
        let mut r = Ring::new(100_000_000);
        for s in 0..20 {
            r.push(pkt(s, s % 4 == 0)); // GOP de 4 s
        }
        let snap = r.snapshot_last(6_000_000); // coupe à t=13 -> image clé de t=12
        assert_eq!(snap[0].pts_us, 12_000_000);
        assert_eq!(r.snapshot_last(1_000_000_000).len(), 20);
    }

    #[test]
    fn evicts_whole_gops_and_keeps_span() {
        let mut r = Ring::new(5_000_000);
        for s in 0..20 {
            r.push(pkt(s, s % 2 == 0)); // GOP de 2 s
        }
        let snap = r.snapshot();
        assert!(snap[0].keyframe);
        let span = snap.last().unwrap().pts_us - snap[0].pts_us;
        assert!(span >= 5_000_000 && span <= 7_000_000, "span = {span}");
        assert_eq!(r.bytes(), snap.len() * 10);
    }

    #[test]
    fn ring_memory_stays_bounded_over_long_session() {
        let mut r = Ring::new(10_000_000);
        let mut peak = 0;
        for s in 0..10_000 {
            r.push(pkt(s, s % 2 == 0));
            peak = peak.max(r.bytes());
        }
        assert!(peak <= 12 * 10, "peak = {peak}");
    }

    #[test]
    fn pacer_pts_ignore_wakeup_jitter() {
        let mut p = Pacer::new(166_666, 1_000);
        let pts: Vec<i64> = [1_000 + 4_000, 1_000 + 166_666 + 9_000, 1_000 + 2 * 166_666 + 1].iter().map(|&n| p.emit(n)).collect();
        assert_eq!(pts, vec![1_000, 167_666, 334_332]);
    }

    #[test]
    fn pacer_resyncs_after_long_stall() {
        let mut p = Pacer::new(100, 0);
        assert_eq!(p.emit(1_000), 1_000);
        assert_eq!(p.due(), 1_100);
    }

    #[test]
    fn pcm_render_fills_gaps_and_ignores_jitter() {
        let mut r = PcmRing::new(10_000_000);
        let ten_ms = vec![1i16; 480 * CHANNELS];
        r.push(0, &ten_ms);
        r.push(10_500, &ten_ms); // 0,5 ms de gigue : contigu
        r.push(100_000, &ten_ms); // vrai trou : silence de 79,x ms
        let out = r.render(0, 200_000);
        assert_eq!(out.len(), 9600 * CHANNELS);
        assert!(out[..960 * CHANNELS].iter().all(|&x| x == 1));
        assert!(out[960 * CHANNELS..4800 * CHANNELS].iter().all(|&x| x == 0));
        assert!(out[4800 * CHANNELS..5280 * CHANNELS].iter().all(|&x| x == 1));
    }

    #[test]
    fn pcm_render_trims_before_start() {
        let mut r = PcmRing::new(10_000_000);
        r.push(0, &vec![7i16; 960 * CHANNELS]); // 20 ms
        let out = r.render(10_000, 20_000);
        assert_eq!(out.len(), 480 * CHANNELS);
        assert!(out.iter().all(|&x| x == 7));
    }
}
