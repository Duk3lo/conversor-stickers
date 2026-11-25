//! Fix for the black window of WebKitGTK with the NVIDIA driver on Wayland: without the DMABUF
//! renderer the webview paints correctly.
//!
//! `std::env::set_var` is `unsafe` in edition 2024 and the workspace forbids it, so the app
//! replaces itself (`exec`, same pid) with the variable set. That works the same for the `.deb`,
//! the AppImage and a binary launched by hand.

use std::ffi::OsStr;

pub(crate) const DMABUF_VARIABLE: &str = "WEBKIT_DISABLE_DMABUF_RENDERER";

pub(crate) fn should_disable_dmabuf(current_value: Option<&OsStr>) -> bool {
    current_value.is_none()
}

#[cfg(target_os = "linux")]
pub(crate) fn prepare_webview() {
    use std::os::unix::process::CommandExt;
    use std::process::Command;

    if !should_disable_dmabuf(std::env::var_os(DMABUF_VARIABLE).as_deref()) {
        return;
    }
    let executable = match std::env::current_exe() {
        Ok(executable) => executable,
        Err(error) => {
            tracing::warn!(%error, "no se pudo localizar el ejecutable para desactivar DMABUF");
            return;
        }
    };
    let error = Command::new(executable)
        .args(std::env::args_os().skip(1))
        .env(DMABUF_VARIABLE, "1")
        .exec();
    tracing::warn!(%error, "no se pudo relanzar la app sin DMABUF; sigue sin el arreglo");
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn prepare_webview() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_disabled_if_variable_is_not_defined() {
        assert!(should_disable_dmabuf(None));
        assert!(!should_disable_dmabuf(Some(OsStr::new("1"))));
        assert!(!should_disable_dmabuf(Some(OsStr::new("0"))));
        assert!(!should_disable_dmabuf(Some(OsStr::new(""))));
    }
}
