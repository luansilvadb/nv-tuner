use windows::{
    Win32::System::{
        Com::{
            CLSCTX_INPROC_SERVER, CoCreateInstance, CoSetProxyBlanket, EOAC_NONE,
            RPC_C_AUTHN_LEVEL_CALL, RPC_C_IMP_LEVEL_IMPERSONATE,
        },
        Variant::{VariantGetElementCount, VariantToUInt32Array},
        Wmi::{
            CIM_FLAG_ARRAY, CIM_UINT32, IWbemClassObject, IWbemLocator,
            WBEM_FLAG_CONNECT_USE_MAX_WAIT, WBEM_FLAG_FORWARD_ONLY, WBEM_FLAG_RETURN_IMMEDIATELY,
            WbemLocator,
        },
    },
    core::{BSTR, PCWSTR, VARIANT, w},
};

use super::DeviceGuard;
use crate::com::Com;

pub(super) fn device_guard() -> Option<DeviceGuard> {
    let _com = Com::iniciar_estrito()?;
    let objeto = consultar()?;
    let vbs = propriedade(
        &objeto,
        w!("VirtualizationBasedSecurityStatus"),
        CIM_UINT32.0,
    )
    .and_then(|valor| u32::try_from(&valor).ok())
    .filter(|&v| v <= 2);
    let hvci = lista(&objeto, w!("SecurityServicesRunning")).map(|lista| lista.contains(&2));
    let iommu = lista(&objeto, w!("AvailableSecurityProperties")).map(|lista| lista.contains(&3));
    Some(DeviceGuard { vbs, hvci, iommu })
}

fn consultar() -> Option<IWbemClassObject> {
    // SAFETY: COM foi inicializado nesta thread. As interfaces e BSTRs sao
    // gerenciadas pelo crate windows. A consulta e local e somente leitura.
    unsafe {
        let localizador: IWbemLocator =
            CoCreateInstance(&WbemLocator, None, CLSCTX_INPROC_SERVER).ok()?;
        let vazio = BSTR::new();
        let servico = localizador
            .ConnectServer(
                &BSTR::from(r"ROOT\Microsoft\Windows\DeviceGuard"),
                &vazio,
                &vazio,
                &vazio,
                WBEM_FLAG_CONNECT_USE_MAX_WAIT.0,
                &vazio,
                None,
            )
            .ok()?;
        // RPC_C_AUTHN_WINNT = 10 e RPC_C_AUTHZ_NONE = 0.
        CoSetProxyBlanket(
            &servico,
            10,
            0,
            PCWSTR::null(),
            RPC_C_AUTHN_LEVEL_CALL,
            RPC_C_IMP_LEVEL_IMPERSONATE,
            None,
            EOAC_NONE,
        )
        .ok()?;
        let linhas = servico.ExecQuery(
            &BSTR::from("WQL"),
            &BSTR::from("SELECT VirtualizationBasedSecurityStatus, SecurityServicesRunning, AvailableSecurityProperties FROM Win32_DeviceGuard"),
            WBEM_FLAG_FORWARD_ONLY | WBEM_FLAG_RETURN_IMMEDIATELY,
            None,
        ).ok()?;
        let mut unica = [None];
        let mut total = 0;
        linhas.Next(2_000, &mut unica, &mut total).ok().ok()?;
        if total != 1 {
            return None;
        }
        unica[0].take()
    }
}

fn propriedade(objeto: &IWbemClassObject, nome: PCWSTR, tipo: i32) -> Option<VARIANT> {
    let mut valor = VARIANT::default();
    let mut tipo_lido = 0;
    // SAFETY: o nome e estatico, e ambas as saidas estao inicializadas e gravaveis.
    unsafe { objeto.Get(nome, 0, &mut valor, Some(&mut tipo_lido), None) }.ok()?;
    (tipo_lido == tipo && !valor.is_empty()).then_some(valor)
}

fn lista(objeto: &IWbemClassObject, nome: PCWSTR) -> Option<Vec<u32>> {
    let valor = propriedade(objeto, nome, CIM_UINT32.0 | CIM_FLAG_ARRAY.0)?;
    // SAFETY: VARIANT valido retornado por WMI com tipo CIM uint32[].
    let elementos = unsafe { VariantGetElementCount(&valor) };
    if elementos > 64 {
        return None;
    }
    if elementos == 0 {
        return Some(Vec::new());
    }
    let mut itens = vec![0; elementos as usize];
    let mut lidos = 0;
    // SAFETY: o slice e o contador sao gravaveis e o VARIANT esta vivo.
    unsafe { VariantToUInt32Array(&valor, &mut itens, &mut lidos) }.ok()?;
    (lidos == elementos).then_some(itens)
}
