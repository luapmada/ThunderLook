//! `encode(parse(b)) == b` pour les blobs réels de `tests/fixtures/recur_*.hex` (une ligne : étiquette<TAB>hexadécimal).
use thunderlook_calendar::recur::Recurrence;

fn fixtures() -> Vec<(String, String, Vec<u8>)> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut out = Vec::new();
    let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|f| f.unwrap().path()).filter(|p| p.file_name().unwrap().to_string_lossy().starts_with("recur_") && p.extension().is_some_and(|x| x == "hex")).collect();
    files.sort();
    for f in files {
        for (n, line) in std::fs::read_to_string(&f).unwrap().lines().enumerate() {
            let Some((label, hex)) = line.split_once('\t').filter(|_| !line.starts_with('#')) else { continue };
            let blob = (0..hex.len()).step_by(2).map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap()).collect();
            out.push((format!("{}:{}", f.file_name().unwrap().to_string_lossy(), n + 1), label.to_string(), blob));
        }
    }
    out
}

/// Fixtures dont l'encodage diffère légitimement du blob d'origine : `0xFFFFFFFF` (« jamais » à l'ancienne) est réécrit
/// en `0x2023`, la valeur que [MS-OXOCAL] définit et qu'écrit `encode`. Le sens (`parse`) reste identique.
const LEGIT: &[(&str, &str)] = &[
    ("recur_reels.hex:9", "type de fin 0xFFFFFFFF (jamais, ancienne forme d'Outlook) réécrit en 0x2023"),
    ("recur_reels.hex:12", "type de fin 0xFFFFFFFF (jamais, ancienne forme d'Outlook) réécrit en 0x2023"),
];

#[test]
fn encode_parse_is_identity_except_listed() {
    let all = fixtures();
    assert!(all.len() >= 20, "{} fixtures", all.len());
    let mut with_exceptions = 0;
    for (at, label, blob) in &all {
        let r = Recurrence::parse(blob).unwrap_or_else(|e| panic!("{at} {label} : {e}"));
        with_exceptions += usize::from(!r.exceptions.is_empty());
        let enc = r.encode();
        if enc == *blob {
            assert!(LEGIT.iter().all(|(l, _)| l != at), "{at} est listé comme différent mais redonne le blob");
            continue;
        }
        let why = LEGIT.iter().find(|(l, _)| l == at).unwrap_or_else(|| panic!("{at} {label} : encode(parse(b)) != b\n  blob {}\n  enc  {}", hex(blob), hex(&enc)));
        assert_eq!(Recurrence::parse(&enc).unwrap(), r, "{at} ({}) : le sens doit rester le même", why.1);
    }
    assert!(with_exceptions >= 5, "{with_exceptions} séries avec exception");
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}
