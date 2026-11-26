//! Políticas puras: decisão de instalar (U-17..U-19), códigos do msiexec (U-36..U-38),
//! agendamento e espera (U-01, U-02) e URL permitida (U-04).

use std::time::Duration;

use crate::HOSTS_PERMITIDOS;
use crate::versao::Versao;

// ---------------------------------------------------------------- decisão (U-17..U-19)

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decisao {
    Instalar,
    /// `versao ≤ instalada`.
    NaoMaiorQueInstalada,
    /// `versao < maior_vista` (antidowngrade/replay).
    AbaixoDaMaiorVista,
    Abandonada,
}

/// Instala só se `versao > instalada`, `versao ≥ max(maior_vista, instalada)` e não
/// abandonada. Igualdade com `maior_vista` permite retomar um download interrompido.
pub fn decidir_instalacao(
    versao: Versao,
    instalada: Versao,
    maior_vista: Option<Versao>,
    abandonadas: &[Versao],
) -> Decisao {
    let piso = maior_vista.map_or(instalada, |m| m.max(instalada));
    if versao <= instalada {
        Decisao::NaoMaiorQueInstalada
    } else if versao < piso {
        Decisao::AbaixoDaMaiorVista
    } else if abandonadas.contains(&versao) {
        Decisao::Abandonada
    } else {
        Decisao::Instalar
    }
}

// ---------------------------------------------------------------- msiexec (U-36..U-38)

/// Falhas da mesma versão até abandoná-la.
pub const FALHAS_PARA_ABANDONAR: u8 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultadoMsi {
    /// 0, 3010 (`reinicio_pendente`), 1641 (`aviso`: não deveria ocorrer com `/norestart`).
    Sucesso {
        reinicio_pendente: bool,
        aviso: bool,
    },
    /// 1618: outra instalação em andamento; adia 30 min sem contar falha.
    Ocupado,
    /// 1625: bloqueado por política; abandona a versão já.
    Politica,
    /// Demais códigos (inclusive `-1`, resultado desconhecido): contam falha.
    Falha,
}

pub fn interpretar_codigo_msi(codigo: i64) -> ResultadoMsi {
    match codigo {
        0 => ResultadoMsi::Sucesso {
            reinicio_pendente: false,
            aviso: false,
        },
        3010 => ResultadoMsi::Sucesso {
            reinicio_pendente: true,
            aviso: false,
        },
        1641 => ResultadoMsi::Sucesso {
            reinicio_pendente: true,
            aviso: true,
        },
        1618 => ResultadoMsi::Ocupado,
        1625 => ResultadoMsi::Politica,
        _ => ResultadoMsi::Falha,
    }
}

// ---------------------------------------------------------------- agendamento (U-01, U-02)

pub const MINUTO: u64 = 60;
pub const HORA: u64 = 3600;
pub const ESPERA_REDE_MIN_S: u64 = 30 * MINUTO;
pub const ESPERA_REDE_MAX_S: u64 = 6 * HORA;
pub const ESPERA_LIMITE_MAX_S: u64 = 24 * HORA;
pub const REAVALIAR_PORTAO: Duration = Duration::from_secs(60);
pub const REAVALIAR_MSI_OCUPADO: Duration = Duration::from_secs(30 * MINUTO);
pub const RELER_LOG: Duration = Duration::from_secs(60);
pub const PRONTA_MAX_S: u64 = 24 * HORA;
pub const INSTALANDO_MAX_S: u64 = 2 * HORA;
/// `proxima_verificacao_unix` persistida mais longe que isso é descartada.
pub const PROXIMA_PERSISTIDA_MAX_S: u64 = 7 * HORA;

/// `aleatorio` vem do RNG do sistema (injetado); uniforme em `[min, max]`.
fn uniforme(aleatorio: u64, min: u64, max: u64) -> u64 {
    min + aleatorio % (max - min + 1)
}

/// Primeira verificação: 5–15 min após o serviço iniciar.
pub fn atraso_inicial(aleatorio: u64) -> Duration {
    Duration::from_secs(uniforme(aleatorio, 5 * MINUTO, 15 * MINUTO))
}

/// Verificações regulares: 6 h ± 30 min.
pub fn intervalo_regular(aleatorio: u64) -> Duration {
    Duration::from_secs(uniforme(
        aleatorio,
        5 * HORA + 30 * MINUTO,
        6 * HORA + 30 * MINUTO,
    ))
}

/// Espera exponencial de rede: 30 min, 60, 120, 240, 360 (teto 6 h).
pub fn proxima_espera_rede(anterior_s: u64) -> u64 {
    anterior_s
        .saturating_mul(2)
        .clamp(ESPERA_REDE_MIN_S, ESPERA_REDE_MAX_S)
}

/// 403/429: max(espera exponencial, `Retry-After`, `x-ratelimit-reset`), teto 24 h.
pub fn espera_limite(exponencial_s: u64, retry_after_s: Option<u64>, reset_s: Option<u64>) -> u64 {
    exponencial_s
        .max(retry_after_s.unwrap_or(0))
        .max(reset_s.unwrap_or(0))
        .min(ESPERA_LIMITE_MAX_S)
}

/// `Retry-After`: segundos ou data HTTP (IMF-fixdate), relativo a `agora_unix`.
pub fn interpretar_retry_after(valor: &str, agora_unix: u64) -> Option<u64> {
    let v = valor.trim();
    if !v.is_empty() && v.len() <= 10 && v.bytes().all(|b| b.is_ascii_digit()) {
        return v.parse().ok();
    }
    data_http_unix(v).map(|t| t.saturating_sub(agora_unix))
}

/// `x-ratelimit-reset` (época Unix em segundos) → segundos a partir de agora.
pub fn interpretar_ratelimit_reset(valor: &str, agora_unix: u64) -> Option<u64> {
    let t: u64 = valor.trim().parse().ok()?;
    Some(t.saturating_sub(agora_unix))
}

/// `Sun, 06 Nov 1994 08:49:37 GMT`.
fn data_http_unix(s: &str) -> Option<u64> {
    let p: Vec<&str> = s.split_ascii_whitespace().collect();
    if p.len() != 6 || p[5] != "GMT" || !p[0].ends_with(',') {
        return None;
    }
    let dia: u64 = p[1].parse().ok()?;
    const MESES: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let mes = MESES.iter().position(|m| *m == p[2])? as u64 + 1;
    let ano: i64 = p[3].parse().ok()?;
    let h: Vec<u64> = p[4]
        .split(':')
        .map(|x| x.parse().ok())
        .collect::<Option<_>>()?;
    if h.len() != 3 || h[0] > 23 || h[1] > 59 || h[2] > 60 || !(1..=31).contains(&dia) {
        return None;
    }
    // Dias desde 1970-01-01 (algoritmo de Howard Hinnant).
    let y = if mes <= 2 { ano - 1 } else { ano };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let m = mes as i64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + dia as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let dias = era * 146_097 + doe - 719_468;
    let t = dias * 86_400 + (h[0] * 3600 + h[1] * 60 + h[2]) as i64;
    u64::try_from(t).ok()
}

/// `proxima_verificacao_unix` persistida só é usada se não está no passado nem a mais
/// de 7 h no futuro (relógio ajustado); senão é recalculada.
pub fn proxima_persistida_valida(proxima_unix: u64, agora_unix: u64) -> bool {
    proxima_unix >= agora_unix && proxima_unix - agora_unix <= PROXIMA_PERSISTIDA_MAX_S
}

/// Agendador em relógio monotônico. `I` é o instante (em produção `std::time::Instant`;
/// nos testes, qualquer tipo com aritmética de `Duration`).
#[derive(Debug, Clone, Copy)]
pub struct Agendador<I> {
    proxima: I,
}

impl<I> Agendador<I>
where
    I: Copy + PartialOrd + std::ops::Add<Duration, Output = I>,
{
    /// Ao iniciar o serviço: 5–15 min (a persistida é só informativa).
    pub fn ao_iniciar(agora: I, aleatorio: u64) -> Self {
        Agendador {
            proxima: agora + atraso_inicial(aleatorio),
        }
    }

    pub fn agendar(&mut self, agora: I, espera: Duration) {
        self.proxima = agora + espera;
    }

    pub fn venceu(&self, agora: I) -> bool {
        agora >= self.proxima
    }

    pub fn proxima(&self) -> I {
        self.proxima
    }
}

// ---------------------------------------------------------------- URL (U-04)

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErroUrl {
    NaoHttps,
    Credenciais,
    Porta,
    HostNaoPermitido(String),
    Malformada,
}

/// URL já conferida: `https://<host permitido><caminho>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UrlPermitida {
    pub host: String,
    /// Caminho com consulta (começa por `/`).
    pub caminho: String,
}

/// Confere uma URL absoluta (inclusive `Location` de redirecionamento).
pub fn politica_url(url: &str) -> Result<UrlPermitida, ErroUrl> {
    if url
        .bytes()
        .any(|b| b.is_ascii_control() || b == b' ' || b == b'\\')
    {
        return Err(ErroUrl::Malformada);
    }
    let (esquema, resto) = url.split_once("://").ok_or(ErroUrl::Malformada)?;
    if !esquema.eq_ignore_ascii_case("https") {
        return Err(ErroUrl::NaoHttps);
    }
    let fim = resto.find(['/', '?', '#']).unwrap_or(resto.len());
    let (autoridade, caminho) = resto.split_at(fim);
    if autoridade.contains('@') {
        return Err(ErroUrl::Credenciais);
    }
    let host = match autoridade.rsplit_once(':') {
        Some((h, "443")) => h,
        Some(_) => return Err(ErroUrl::Porta),
        None => autoridade,
    };
    let host = host.to_ascii_lowercase();
    if !HOSTS_PERMITIDOS.contains(&host.as_str()) {
        return Err(ErroUrl::HostNaoPermitido(host));
    }
    let caminho = caminho.split('#').next().unwrap_or("");
    let caminho = if caminho.starts_with('/') {
        caminho.to_string()
    } else {
        format!("/{caminho}")
    };
    Ok(UrlPermitida { host, caminho })
}

/// Resolve um `Location` (absoluto ou relativo à raiz do host atual) e aplica a política.
pub fn resolver_redirecionamento(
    atual: &UrlPermitida,
    location: &str,
) -> Result<UrlPermitida, ErroUrl> {
    if location.starts_with('/') && !location.starts_with("//") {
        politica_url(&format!("https://{}{location}", atual.host))
    } else {
        politica_url(location)
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn v(s: &str) -> Versao {
        s.parse().unwrap()
    }

    #[test]
    fn tabela_de_decisao() {
        let i = v("0.1.0");
        let casos = [
            ("0.1.0", None, Decisao::NaoMaiorQueInstalada),
            ("0.0.9", None, Decisao::NaoMaiorQueInstalada),
            ("0.2.0", None, Decisao::Instalar),
            ("0.2.0", Some("0.2.0"), Decisao::Instalar), // retomada
            ("0.2.0", Some("0.3.0"), Decisao::AbaixoDaMaiorVista), // replay
            ("0.3.0", Some("0.2.0"), Decisao::Instalar),
            ("0.2.0", Some("0.0.5"), Decisao::Instalar), // piso = instalada
        ];
        for (c, mv, esperado) in casos {
            assert_eq!(decidir_instalacao(v(c), i, mv.map(v), &[]), esperado, "{c}");
        }
        assert_eq!(
            decidir_instalacao(v("0.2.0"), i, None, &[v("0.2.0")]),
            Decisao::Abandonada
        );
        // U-19: versão maior que a abandonada volta a ser tentada.
        assert_eq!(
            decidir_instalacao(v("0.2.1"), i, Some(v("0.2.0")), &[v("0.2.0")]),
            Decisao::Instalar
        );
    }

    #[test]
    fn codigos_msi() {
        assert!(matches!(
            interpretar_codigo_msi(0),
            ResultadoMsi::Sucesso {
                reinicio_pendente: false,
                ..
            }
        ));
        assert!(matches!(
            interpretar_codigo_msi(3010),
            ResultadoMsi::Sucesso {
                reinicio_pendente: true,
                aviso: false
            }
        ));
        assert!(matches!(
            interpretar_codigo_msi(1641),
            ResultadoMsi::Sucesso { aviso: true, .. }
        ));
        assert_eq!(interpretar_codigo_msi(1618), ResultadoMsi::Ocupado);
        assert_eq!(interpretar_codigo_msi(1625), ResultadoMsi::Politica);
        for c in [1603, 1619, 1602, -1, 5] {
            assert_eq!(interpretar_codigo_msi(c), ResultadoMsi::Falha);
        }
    }

    #[test]
    fn jitter_dentro_dos_limites() {
        for a in [0, 1, 599, 600, u64::MAX, 12345678] {
            let i = atraso_inicial(a).as_secs();
            assert!((300..=900).contains(&i));
            let r = intervalo_regular(a).as_secs();
            assert!((19800..=23400).contains(&r));
        }
        assert_eq!(atraso_inicial(0).as_secs(), 300);
        assert_eq!(atraso_inicial(600).as_secs(), 900);
    }

    #[test]
    fn espera_exponencial() {
        let mut e = 0;
        let mut seq = vec![];
        for _ in 0..6 {
            e = proxima_espera_rede(e);
            seq.push(e / 60);
        }
        assert_eq!(seq, [30, 60, 120, 240, 360, 360]);
        assert_eq!(espera_limite(1800, Some(7200), None), 7200);
        assert_eq!(espera_limite(1800, None, Some(100)), 1800);
        assert_eq!(espera_limite(1800, Some(10 * 86400), None), 86400);
    }

    #[test]
    fn retry_after() {
        assert_eq!(interpretar_retry_after("120", 0), Some(120));
        // 1994-11-06T08:49:37Z = 784111777
        assert_eq!(
            interpretar_retry_after("Sun, 06 Nov 1994 08:49:37 GMT", 784_111_700),
            Some(77)
        );
        assert_eq!(
            interpretar_retry_after("Sun, 06 Nov 1994 08:49:37 GMT", u64::MAX),
            Some(0)
        );
        assert_eq!(interpretar_retry_after("amanhã", 0), None);
        assert_eq!(interpretar_ratelimit_reset("1000", 400), Some(600));
    }

    #[test]
    fn relogio_monotonico_e_persistida() {
        let mut a = Agendador::ao_iniciar(Duration::ZERO, 0);
        assert!(!a.venceu(Duration::from_secs(299)));
        assert!(a.venceu(Duration::from_secs(300)));
        a.agendar(Duration::from_secs(300), intervalo_regular(0));
        assert!(!a.venceu(Duration::from_secs(301)));
        // Relógio de parede saltando anos para trás ou para frente: persistida descartada.
        assert!(!proxima_persistida_valida(1_000, 2_000_000_000));
        assert!(!proxima_persistida_valida(2_000_000_000, 1_000));
        assert!(proxima_persistida_valida(1_000 + 6 * HORA, 1_000));
        assert!(!proxima_persistida_valida(1_000 + 7 * HORA + 1, 1_000));
    }

    #[test]
    fn url_permitida() {
        let u = politica_url(crate::URL_MANIFESTO).unwrap();
        assert_eq!(u.host, "github.com");
        assert!(politica_url("https://release-assets.githubusercontent.com:443/a?b=1").is_ok());
        assert_eq!(politica_url("http://github.com/x"), Err(ErroUrl::NaoHttps));
        assert_eq!(
            politica_url("https://u:p@github.com/x"),
            Err(ErroUrl::Credenciais)
        );
        assert_eq!(
            politica_url("https://github.com:8443/x"),
            Err(ErroUrl::Porta)
        );
        assert!(matches!(
            politica_url("https://evil.githubusercontent.com/x"),
            Err(ErroUrl::HostNaoPermitido(_))
        ));
        assert!(matches!(
            politica_url("https://github.com.evil.com/x"),
            Err(ErroUrl::HostNaoPermitido(_))
        ));
        assert_eq!(
            resolver_redirecionamento(&u, "/outro").unwrap().caminho,
            "/outro"
        );
        assert!(resolver_redirecionamento(&u, "//evil.com/x").is_err());
    }
}
