use crate::traits::{CompteursProcessus, InfosMachine};

fn lire(chemin: &str) -> Option<String> {
    std::fs::read_to_string(chemin).ok()
}

pub fn compteurs() -> Option<CompteursProcessus> {
    let (prive, ws, pic_ws) = memoire_de_status(&lire("/proc/self/status")?)?;
    let cpu_ms = lire("/proc/self/stat")
        .as_deref()
        .and_then(cpu_ms_de_stat)
        .unwrap_or(0);
    Some(CompteursProcessus {
        prive,
        ws,
        pic_ws,
        cpu_ms,
    })
}

pub fn machine() -> InfosMachine {
    let systeme = lire("/etc/os-release")
        .as_deref()
        .and_then(nom_os_release)
        .unwrap_or_else(|| "Linux".into());
    let noyau = lire("/proc/sys/kernel/osrelease").map(|v| v.trim().to_string());
    InfosMachine {
        os: Some(match noyau {
            Some(noyau) => format!("{systeme} (noyau {noyau})"),
            None => systeme,
        }),
        cpu: lire("/proc/cpuinfo")
            .as_deref()
            .and_then(|c| valeur_cpuinfo(c, "model name")),
        coeurs: std::thread::available_parallelism().map_or(0, |n| n.get()),
        ram_mo: lire("/proc/meminfo").as_deref().and_then(ram_mo_de_meminfo),
        gpu: Vec::new(),
    }
}

const KO: u64 = 1024;
const TOPS_PAR_SECONDE_LINUX: u64 = 100;

fn champ_ko(texte: &str, nom: &str) -> Option<u64> {
    texte.lines().find_map(|ligne| {
        let reste = ligne.strip_prefix(nom)?.strip_prefix(':')?;
        reste.split_whitespace().next()?.parse::<u64>().ok()
    })
}

fn memoire_de_status(status: &str) -> Option<(u64, u64, u64)> {
    let ws = champ_ko(status, "VmRSS")? * KO;
    let pic = champ_ko(status, "VmHWM").map_or(ws, |v| v * KO);
    let prive = (champ_ko(status, "RssAnon")? + champ_ko(status, "VmSwap").unwrap_or(0)) * KO;
    Some((prive, ws, pic.max(ws)))
}

fn cpu_ms_de_stat(stat: &str) -> Option<u64> {
    let apres = &stat[stat.rfind(')')? + 1..];
    let champs: Vec<&str> = apres.split_whitespace().collect();
    let utilisateur = champs.get(11)?.parse::<u64>().ok()?;
    let noyau = champs.get(12)?.parse::<u64>().ok()?;
    Some((utilisateur + noyau) * 1000 / TOPS_PAR_SECONDE_LINUX)
}

fn valeur_cpuinfo(cpuinfo: &str, cle: &str) -> Option<String> {
    cpuinfo.lines().find_map(|ligne| {
        let (nom, valeur) = ligne.split_once(':')?;
        (nom.trim() == cle)
            .then(|| valeur.trim().to_string())
            .filter(|v| !v.is_empty())
    })
}

fn nom_os_release(os_release: &str) -> Option<String> {
    os_release.lines().find_map(|ligne| {
        let valeur = ligne.strip_prefix("PRETTY_NAME=")?;
        Some(valeur.trim().trim_matches('"').to_string()).filter(|v| !v.is_empty())
    })
}

fn ram_mo_de_meminfo(meminfo: &str) -> Option<u64> {
    champ_ko(meminfo, "MemTotal").map(|ko| ko / KO)
}

#[cfg(test)]
mod tests {
    use super::*;

    const STATUS: &str = "Name:\testompe-natif\nVmHWM:\t  204800 kB\nVmRSS:\t  153600 kB\nRssAnon:\t   92160 kB\nRssFile:\t   61440 kB\nVmSwap:\t    1024 kB\n";

    #[test]
    fn lit_la_memoire_du_processus_dans_status() {
        assert_eq!(
            memoire_de_status(STATUS),
            Some((93_184 * KO, 153_600 * KO, 204_800 * KO))
        );
        assert_eq!(memoire_de_status("Name:\tx\n"), None);
    }

    #[test]
    fn lit_le_temps_cpu_dans_stat_meme_avec_une_parenthese_dans_le_nom() {
        let stat = "4242 (estompe (natif)) S 1 4242 4242 0 -1 4194304 1000 0 0 0 250 50 0 0 20 0 30 0 100 0 0";
        assert_eq!(cpu_ms_de_stat(stat), Some(3_000));
        assert_eq!(cpu_ms_de_stat("4242 (x) S 1"), None);
    }

    #[test]
    fn lit_le_processeur_la_memoire_et_le_systeme() {
        let cpuinfo = "processor\t: 0\nmodel name\t: AMD Ryzen 7 5800X 8-Core Processor\n\nprocessor\t: 1\nmodel name\t: AMD Ryzen 7 5800X 8-Core Processor\n";
        assert_eq!(
            valeur_cpuinfo(cpuinfo, "model name").as_deref(),
            Some("AMD Ryzen 7 5800X 8-Core Processor")
        );
        assert_eq!(
            ram_mo_de_meminfo("MemTotal:       16473472 kB\n"),
            Some(16_087)
        );
        assert_eq!(
            nom_os_release("NAME=\"Ubuntu\"\nPRETTY_NAME=\"Ubuntu 24.04.3 LTS\"\n").as_deref(),
            Some("Ubuntu 24.04.3 LTS")
        );
    }
}
