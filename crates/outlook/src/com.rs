//! Liaison tardive (`IDispatch`) avec le modèle objet d'Outlook : propriétés, méthodes et conversions de `VARIANT`.
//! Tout se fait sur le fil qui a appelé [`with_com`].

use std::mem::ManuallyDrop;

use windows::{
    Win32::System::{
        Com::{
            CLSIDFromProgID, COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize, DISPATCH_FLAGS, DISPATCH_METHOD, DISPATCH_PROPERTYGET,
            DISPATCH_PROPERTYPUT, DISPPARAMS, EXCEPINFO, IDispatch, SAFEARRAY,
        },
        Ole::{DISPID_PROPERTYPUT, GetActiveObject, SafeArrayAccessData, SafeArrayGetLBound, SafeArrayGetUBound, SafeArrayUnaccessData},
        Variant::{VARENUM, VARIANT, VARIANT_0, VARIANT_0_0, VARIANT_0_0_0, VT_ARRAY, VT_BOOL, VT_BSTR, VT_DATE, VT_EMPTY, VT_I2, VT_I4, VT_UI1, VT_VARIANT},
    },
    core::{BSTR, GUID, HSTRING, Interface, PCWSTR},
};

pub type Result<T> = std::result::Result<T, String>;

/// Exécute `f` sur un fil dédié, avec COM initialisé (appartement monothread), et rend son résultat.
pub fn with_com<T: Send + 'static>(f: impl FnOnce() -> Result<T> + Send + 'static) -> Result<T> {
    std::thread::spawn(move || {
        let init = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        if init.is_err() {
            return Err(format!("initialisation de COM : {init:?}"));
        }
        let r = f();
        unsafe { CoUninitialize() };
        r
    })
    .join()
    .map_err(|_| "le fil d'Outlook s'est arrêté".to_string())?
}

/// Objet COM manipulé par nom.
#[derive(Clone)]
pub struct Disp(pub IDispatch);

/// L'instance d'Outlook déjà ouverte ; jamais lancée d'ici (Outlook lancé ainsi reste parfois figé sans fenêtre).
pub fn outlook() -> Result<Disp> {
    let clsid: GUID = unsafe { CLSIDFromProgID(&HSTRING::from("Outlook.Application")) }.map_err(|_| "Outlook n'est pas installé".to_string())?;
    let mut unk = None;
    unsafe { GetActiveObject(&clsid, None, &mut unk) }.map_err(|_| "Outlook n'est pas ouvert : le calendrier commun passe par Outlook.".to_string())?;
    let unk = unk.ok_or("Outlook n'est pas ouvert")?;
    Ok(Disp(unk.cast::<IDispatch>().map_err(|e| e.to_string())?))
}

impl Disp {
    fn dispid(&self, name: &str) -> Result<i32> {
        let wide = HSTRING::from(name);
        let names = [PCWSTR(wide.as_ptr())];
        let mut id = 0;
        unsafe { self.0.GetIDsOfNames(&GUID::zeroed(), names.as_ptr(), 1, 0x0400, &mut id) }.map_err(|e| format!("{name} : {e}"))?;
        Ok(id)
    }

    fn invoke(&self, name: &str, flags: DISPATCH_FLAGS, args: &[VARIANT]) -> Result<VARIANT> {
        let id = self.dispid(name)?;
        // Les arguments se passent dans l'ordre inverse.
        let mut rev: Vec<VARIANT> = args.iter().rev().cloned().collect();
        let mut put = DISPID_PROPERTYPUT;
        let params = DISPPARAMS {
            rgvarg: if rev.is_empty() { std::ptr::null_mut() } else { rev.as_mut_ptr() },
            rgdispidNamedArgs: if flags == DISPATCH_PROPERTYPUT { &mut put } else { std::ptr::null_mut() },
            cArgs: rev.len() as u32,
            cNamedArgs: u32::from(flags == DISPATCH_PROPERTYPUT),
        };
        let mut result = VARIANT::default();
        let mut excep = EXCEPINFO::default();
        let r = unsafe { self.0.Invoke(id, &GUID::zeroed(), 0x0400, flags, &params, Some(&mut result), Some(&mut excep), None) };
        match r {
            Ok(()) => Ok(result),
            Err(e) => {
                let detail = excep.bstrDescription.to_string();
                Err(if detail.is_empty() { format!("{name} : {e}") } else { format!("{name} : {detail}") })
            }
        }
    }

    pub fn get(&self, name: &str) -> Result<VARIANT> {
        self.invoke(name, DISPATCH_PROPERTYGET | DISPATCH_METHOD, &[])
    }

    pub fn get_with(&self, name: &str, args: &[VARIANT]) -> Result<VARIANT> {
        self.invoke(name, DISPATCH_PROPERTYGET | DISPATCH_METHOD, args)
    }

    pub fn call(&self, name: &str, args: &[VARIANT]) -> Result<VARIANT> {
        self.invoke(name, DISPATCH_METHOD, args)
    }

    pub fn put(&self, name: &str, value: VARIANT) -> Result<()> {
        self.invoke(name, DISPATCH_PROPERTYPUT, &[value]).map(|_| ())
    }

    pub fn obj(&self, name: &str) -> Result<Disp> {
        as_disp(&self.get(name)?, name)
    }

    pub fn obj_with(&self, name: &str, args: &[VARIANT]) -> Result<Disp> {
        as_disp(&self.get_with(name, args)?, name)
    }

    pub fn string(&self, name: &str) -> Result<String> {
        Ok(as_string(&self.get(name)?))
    }

    pub fn int(&self, name: &str) -> Result<i64> {
        as_int(&self.get(name)?).ok_or_else(|| format!("{name} : entier attendu"))
    }
}

pub fn as_disp(v: &VARIANT, what: &str) -> Result<Disp> {
    IDispatch::try_from(v).map(Disp).map_err(|_| format!("{what} : objet attendu"))
}

fn vt(v: &VARIANT) -> VARENUM {
    v.vt()
}

pub fn as_string(v: &VARIANT) -> String {
    if vt(v) == VT_BSTR { unsafe { v.Anonymous.Anonymous.Anonymous.bstrVal.to_string() } } else { String::new() }
}

pub fn as_int(v: &VARIANT) -> Option<i64> {
    let x = unsafe { &v.Anonymous.Anonymous.Anonymous };
    match vt(v) {
        VT_I4 => Some(i64::from(unsafe { x.lVal })),
        VT_I2 => Some(i64::from(unsafe { x.iVal })),
        VT_BOOL => Some(i64::from(unsafe { x.boolVal.0 } != 0)),
        VT_UI1 => Some(i64::from(unsafe { x.bVal })),
        _ => i64::try_from(v).ok(),
    }
}

pub fn as_bool(v: &VARIANT) -> Option<bool> {
    as_int(v).map(|i| i != 0)
}

/// Date OLE (jours depuis le 30/12/1899), telle que la donne Outlook.
pub fn as_date(v: &VARIANT) -> Option<f64> {
    (vt(v) == VT_DATE).then(|| unsafe { v.Anonymous.Anonymous.Anonymous.date })
}

pub fn date(d: f64) -> VARIANT {
    VARIANT {
        Anonymous: VARIANT_0 {
            Anonymous: ManuallyDrop::new(VARIANT_0_0 { vt: VT_DATE, wReserved1: 0, wReserved2: 0, wReserved3: 0, Anonymous: VARIANT_0_0_0 { date: d } }),
        },
    }
}

/// Tableau `SAFEARRAY` à une dimension : bornes et pointeur sur les données, rendus le temps de `f`.
fn with_array<T>(v: &VARIANT, f: impl FnOnce(*const u8, usize) -> T) -> Option<T> {
    let psa: *mut SAFEARRAY = unsafe { v.Anonymous.Anonymous.Anonymous.parray };
    if psa.is_null() {
        return None;
    }
    unsafe {
        let (lo, hi) = (SafeArrayGetLBound(psa, 1).ok()?, SafeArrayGetUBound(psa, 1).ok()?);
        let n = (hi - lo + 1).max(0) as usize;
        let mut data = std::ptr::null_mut();
        SafeArrayAccessData(psa, &mut data).ok()?;
        let r = f(data as *const u8, n);
        let _ = SafeArrayUnaccessData(psa);
        Some(r)
    }
}

/// Binaire (`VT_ARRAY | VT_UI1`).
pub fn as_bytes(v: &VARIANT) -> Option<Vec<u8>> {
    if vt(v) != VARENUM(VT_ARRAY.0 | VT_UI1.0) {
        return None;
    }
    with_array(v, |p, n| unsafe { std::slice::from_raw_parts(p, n).to_vec() })
}

/// Chaînes multiples (`VT_ARRAY | VT_VARIANT` ou `VT_ARRAY | VT_BSTR`).
pub fn as_strings(v: &VARIANT) -> Option<Vec<String>> {
    let t = vt(v);
    if t == VARENUM(VT_ARRAY.0 | VT_VARIANT.0) {
        with_array(v, |p, n| unsafe { std::slice::from_raw_parts(p as *const VARIANT, n).iter().map(as_string).collect() })
    } else if t == VARENUM(VT_ARRAY.0 | VT_BSTR.0) {
        with_array(v, |p, n| unsafe { std::slice::from_raw_parts(p as *const BSTR, n).iter().map(|b| b.to_string()).collect() })
    } else {
        None
    }
}

pub fn is_empty(v: &VARIANT) -> bool {
    vt(v) == VT_EMPTY
}
