use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};

use wasapi::{initialize_mta, AudioClient, Direction, SampleType, StreamMode, WaveFormat};

pub const FREQUENCE: u32 = 48_000;

pub fn capturer(
    ouvrir: impl FnOnce() -> Result<AudioClient, String>,
    canaux: usize,
    arret: &AtomicBool,
    pret: impl FnOnce(Result<(), String>),
    mut recu: impl FnMut(&mut VecDeque<u8>) -> bool,
) {
    let demarrer = || -> Result<_, String> {
        initialize_mta().ok().map_err(|e| e.to_string())?;
        let format = WaveFormat::new(32, 32, &SampleType::Float, FREQUENCE as usize, canaux, None);
        let mut client = ouvrir()?;
        client
            .initialize_client(&format, &Direction::Capture, &StreamMode::EventsShared { autoconvert: true, buffer_duration_hns: 0 })
            .map_err(|e| e.to_string())?;
        let evenement = client.set_get_eventhandle().map_err(|e| e.to_string())?;
        let capture = client.get_audiocaptureclient().map_err(|e| e.to_string())?;
        client.start_stream().map_err(|e| e.to_string())?;
        Ok((client, evenement, capture))
    };
    let (client, evenement, capture) = match demarrer() {
        Ok(v) => v,
        Err(e) => return pret(Err(e)),
    };
    pret(Ok(()));
    let mut file = VecDeque::<u8>::new();
    while !arret.load(Ordering::Relaxed) {
        let _ = evenement.wait_for_event(100);
        while capture.get_next_packet_size().ok().flatten().unwrap_or(0) > 0 {
            if capture.read_from_device_to_deque(&mut file).is_err() {
                return;
            }
        }
        if !recu(&mut file) {
            return;
        }
    }
    let _ = client.stop_stream();
}

pub fn pcm16(octets: &[u8]) -> Vec<i16> {
    flottants(octets).map(|f| (f.clamp(-1.0, 1.0) * i16::MAX as f32) as i16).collect()
}

pub fn flottants(octets: &[u8]) -> impl Iterator<Item = f32> + '_ {
    octets.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

#[cfg(test)]
mod tests {
    use super::pcm16;

    #[test]
    fn convertit_et_borne_les_echantillons() {
        let octets: Vec<u8> = [0.0f32, 1.0, -1.0, 2.0, 0.5].iter().flat_map(|f| f.to_le_bytes()).collect();
        assert_eq!(pcm16(&octets), vec![0, i16::MAX, -i16::MAX, i16::MAX, 16383]);
    }
}
