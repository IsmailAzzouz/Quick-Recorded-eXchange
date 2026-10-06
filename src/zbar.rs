#![allow(unsafe_code)] // This module is the audited boundary around the ZBar C API.

use std::{
    ffi::{c_char, c_int, c_uint, c_ulong, c_void},
    path::PathBuf,
    slice,
};

use libloading::Library;
use thiserror::Error;

const ZBAR_QRCODE: c_uint = 64;
const ZBAR_CFG_ENABLE: c_uint = 0;
const Y800: c_ulong = u32::from_le_bytes(*b"Y800") as c_ulong;

type ScannerCreate = unsafe extern "C" fn() -> *mut c_void;
type ScannerDestroy = unsafe extern "C" fn(*mut c_void);
type ScannerSetConfig = unsafe extern "C" fn(*mut c_void, c_uint, c_uint, c_int) -> c_int;
type ImageCreate = unsafe extern "C" fn() -> *mut c_void;
type ImageDestroy = unsafe extern "C" fn(*mut c_void);
type ImageSetFormat = unsafe extern "C" fn(*mut c_void, c_ulong);
type ImageSetSize = unsafe extern "C" fn(*mut c_void, c_uint, c_uint);
type ImageSetData = unsafe extern "C" fn(
    *mut c_void,
    *const c_void,
    c_ulong,
    Option<unsafe extern "C" fn(*mut c_void)>,
);
type ScanImage = unsafe extern "C" fn(*mut c_void, *mut c_void) -> c_int;
type ImageFirstSymbol = unsafe extern "C" fn(*const c_void) -> *const c_void;
type SymbolNext = unsafe extern "C" fn(*const c_void) -> *const c_void;
type SymbolData = unsafe extern "C" fn(*const c_void) -> *const c_char;
type SymbolDataLength = unsafe extern "C" fn(*const c_void) -> c_uint;

#[derive(Clone, Copy)]
struct Api {
    scanner_create: ScannerCreate,
    scanner_destroy: ScannerDestroy,
    scanner_set_config: ScannerSetConfig,
    image_create: ImageCreate,
    image_destroy: ImageDestroy,
    image_set_format: ImageSetFormat,
    image_set_size: ImageSetSize,
    image_set_data: ImageSetData,
    scan_image: ScanImage,
    image_first_symbol: ImageFirstSymbol,
    symbol_next: SymbolNext,
    symbol_data: SymbolData,
    symbol_data_length: SymbolDataLength,
}

#[derive(Debug, Error)]
pub enum ZbarError {
    #[error(
        "ZBar library was not found. Put libzbar-64.dll beside the application or set QR_VIDEO_ZBAR_DLL."
    )]
    LibraryNotFound,
    #[error("could not load ZBar: {0}")]
    Load(#[from] libloading::Error),
    #[error("ZBar could not create a scanner")]
    ScannerCreate,
    #[error("ZBar could not enable QR decoding")]
    EnableQr,
}

/// Safe, single-threaded wrapper around the `ZBar` image scanner.
pub struct ZbarScanner {
    _dependencies: Vec<Library>,
    _library: Library,
    api: Api,
    scanner: *mut c_void,
}

impl ZbarScanner {
    pub fn new() -> Result<Self, ZbarError> {
        let library_path = candidate_paths()
            .into_iter()
            .find(|path| path.is_file())
            .ok_or(ZbarError::LibraryNotFound)?;
        let mut dependencies = Vec::new();
        if let Some(iconv) = library_path
            .parent()
            .map(|folder| folder.join("libiconv.dll"))
            .filter(|path| path.is_file())
        {
            // SAFETY: The Windows ZBar package needs libiconv first. Keeping the handle alive
            // ensures that ZBar's dependency remains loaded for this scanner's full lifetime.
            dependencies.push(unsafe { Library::new(iconv) }?);
        }
        // SAFETY: The path is selected from a user-controlled override or trusted application
        // locations. Its exported functions are resolved below with the signatures in ZBar's C API.
        let library = unsafe { Library::new(library_path) }?;
        // SAFETY: `library` remains owned by this scanner for as long as all copied function pointers
        // may be called. The names and ABI match ZBar 0.23's public zbar.h declarations.
        let api = unsafe {
            Api {
                scanner_create: *library.get(b"zbar_image_scanner_create\0")?,
                scanner_destroy: *library.get(b"zbar_image_scanner_destroy\0")?,
                scanner_set_config: *library.get(b"zbar_image_scanner_set_config\0")?,
                image_create: *library.get(b"zbar_image_create\0")?,
                image_destroy: *library.get(b"zbar_image_destroy\0")?,
                image_set_format: *library.get(b"zbar_image_set_format\0")?,
                image_set_size: *library.get(b"zbar_image_set_size\0")?,
                image_set_data: *library.get(b"zbar_image_set_data\0")?,
                scan_image: *library.get(b"zbar_scan_image\0")?,
                image_first_symbol: *library.get(b"zbar_image_first_symbol\0")?,
                symbol_next: *library.get(b"zbar_symbol_next\0")?,
                symbol_data: *library.get(b"zbar_symbol_get_data\0")?,
                symbol_data_length: *library.get(b"zbar_symbol_get_data_length\0")?,
            }
        };
        // SAFETY: Function pointer comes from the loaded ZBar library and has no arguments.
        let scanner = unsafe { (api.scanner_create)() };
        if scanner.is_null() {
            return Err(ZbarError::ScannerCreate);
        }
        // SAFETY: `scanner` is live. Disable every symbology before enabling QR alone; this avoids
        // needless 1D/PDF417 work and suppresses their decoder warnings on dense QR images.
        let disabled = unsafe { (api.scanner_set_config)(scanner, 0, ZBAR_CFG_ENABLE, 0) };
        // SAFETY: The constants are from zbar.h and configure QR recognition on the same scanner.
        let configured =
            unsafe { (api.scanner_set_config)(scanner, ZBAR_QRCODE, ZBAR_CFG_ENABLE, 1) };
        if disabled != 0 || configured != 0 {
            // SAFETY: `scanner` was created by the matching ZBar create function above.
            unsafe { (api.scanner_destroy)(scanner) };
            return Err(ZbarError::EnableQr);
        }
        Ok(Self {
            _dependencies: dependencies,
            _library: library,
            api,
            scanner,
        })
    }

    pub fn scan(&mut self, pixels: &[u8], width: usize, height: usize) -> Vec<String> {
        let (Ok(width), Ok(height)) = (c_uint::try_from(width), c_uint::try_from(height)) else {
            return Vec::new();
        };
        let expected_len = usize::try_from(width).unwrap_or_default()
            * usize::try_from(height).unwrap_or_default();
        if pixels.len() != expected_len {
            return Vec::new();
        }
        // SAFETY: Function pointer is resolved from the retained ZBar library.
        let image = unsafe { (self.api.image_create)() };
        if image.is_null() {
            return Vec::new();
        }
        let Ok(data_len) = c_ulong::try_from(pixels.len()) else {
            // SAFETY: `image` was allocated immediately above and has not escaped this function.
            unsafe { (self.api.image_destroy)(image) };
            return Vec::new();
        };
        // SAFETY: `image` is live. ZBar borrows `pixels` only for this synchronous scan; no cleanup
        // callback means ZBar never frees Rust-owned memory. The image is destroyed before return.
        unsafe {
            (self.api.image_set_format)(image, Y800);
            (self.api.image_set_size)(image, width, height);
            (self.api.image_set_data)(image, pixels.as_ptr().cast(), data_len, None);
            let _ = (self.api.scan_image)(self.scanner, image);
        }
        let mut results = Vec::new();
        // SAFETY: Symbol pointers are owned by `image` and remain valid until `image_destroy` below.
        let mut symbol = unsafe { (self.api.image_first_symbol)(image) };
        while !symbol.is_null() {
            // SAFETY: ZBar returns a data pointer and byte length for the current valid symbol.
            let data = unsafe { (self.api.symbol_data)(symbol) };
            let len = unsafe { (self.api.symbol_data_length)(symbol) };
            if !data.is_null() {
                // SAFETY: ZBar guarantees `data` has `len` initialized bytes for the symbol lifetime.
                let bytes = unsafe { slice::from_raw_parts(data.cast::<u8>(), len as usize) };
                if let Ok(text) = std::str::from_utf8(bytes) {
                    results.push(text.to_owned());
                }
            }
            // SAFETY: `symbol` remains valid until the image is destroyed after the loop.
            symbol = unsafe { (self.api.symbol_next)(symbol) };
        }
        // SAFETY: `image` was allocated by the matching ZBar create function and is no longer used.
        unsafe { (self.api.image_destroy)(image) };
        results
    }
}

impl Drop for ZbarScanner {
    fn drop(&mut self) {
        // SAFETY: `scanner` was created by this instance and is destroyed before its library drops.
        unsafe { (self.api.scanner_destroy)(self.scanner) };
    }
}

fn candidate_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(path) =
        std::env::var_os("QR_VIDEO_ZBAR_DLL").or_else(|| std::env::var_os("QR_VIDEO_ZBAR_LIB"))
    {
        paths.push(PathBuf::from(path));
    }
    if let Ok(executable) = std::env::current_exe()
        && let Some(folder) = executable.parent()
    {
        paths.push(folder.join("libzbar-64.dll"));
        paths.push(folder.join("libzbar.dll"));
        paths.push(folder.join("libzbar.so"));
        paths.push(folder.join("libzbar.dylib"));
    }
    paths.push(PathBuf::from("libzbar-64.dll"));
    paths.push(PathBuf::from("libzbar.dll"));

    // Check common Windows Python pyzbar installations
    for py_ver in &["Python313", "Python312", "Python311", "Python310"] {
        paths.push(PathBuf::from(format!(
            r"C:\{py_ver}\Lib\site-packages\pyzbar\libzbar-64.dll"
        )));
    }
    if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
        let base = PathBuf::from(local_app_data)
            .join("Programs")
            .join("Python");
        for py_ver in &["Python313", "Python312", "Python311", "Python310"] {
            paths.push(
                base.join(py_ver)
                    .join("Lib")
                    .join("site-packages")
                    .join("pyzbar")
                    .join("libzbar-64.dll"),
            );
        }
    }

    // Unix / Linux / macOS locations
    paths.push(PathBuf::from("/usr/lib/libzbar.so.0"));
    paths.push(PathBuf::from("/usr/lib/libzbar.so"));
    paths.push(PathBuf::from("/usr/lib/x86_64-linux-gnu/libzbar.so.0"));
    paths.push(PathBuf::from("/usr/lib/x86_64-linux-gnu/libzbar.so"));
    paths.push(PathBuf::from("/usr/local/lib/libzbar.so"));
    paths.push(PathBuf::from("/opt/homebrew/lib/libzbar.dylib"));
    paths.push(PathBuf::from("/usr/local/lib/libzbar.dylib"));

    paths
}
