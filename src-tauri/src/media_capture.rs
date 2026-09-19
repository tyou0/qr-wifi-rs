//! Native media-capture permission wiring for the desktop webviews.
//!
//! Tauri delegates camera capture to the OS webview. WebKitGTK disables media
//! streams and denies permission requests unless the embedding app opts in;
//! WebView2 needs an explicit permission decision for its loopback UI. Both
//! handlers are deliberately limited to the exact loopback origin allocated to
//! this process.

pub fn is_trusted_loopback_origin(uri: &str, origin: &str) -> bool {
    uri == origin
        || uri
            .strip_prefix(origin)
            .is_some_and(|remainder| remainder.starts_with('/'))
}

#[cfg(target_os = "linux")]
pub fn configure<R: tauri::Runtime>(
    webview: &tauri::WebviewWindow<R>,
    origin: &str,
) -> tauri::Result<()> {
    use webkit2gtk::{
        glib::prelude::Cast, PermissionRequestExt, SettingsExt, UserMediaPermissionRequest,
        UserMediaPermissionRequestExt, WebViewExt,
    };

    let origin = origin.to_owned();
    webview.with_webview(move |platform_webview| {
        let view = platform_webview.inner();
        if let Some(settings) = WebViewExt::settings(&view) {
            // WebKitGTK defaults this feature off, which leaves getUserMedia
            // unavailable even when a usable camera is present.
            settings.set_enable_media_stream(true);
        }

        // WebKitGTK otherwise denies unhandled media requests. Do not grant
        // permission to arbitrary pages that might later be navigated here.
        view.connect_permission_request(move |view, request| {
            let Some(request) = request.downcast_ref::<UserMediaPermissionRequest>() else {
                return false;
            };
            let current_uri = view.uri().map(|uri| uri.to_string()).unwrap_or_default();
            let wants_device = request.is_for_audio_device() || request.is_for_video_device();
            if wants_device && is_trusted_loopback_origin(&current_uri, &origin) {
                request.allow();
            } else {
                request.deny();
            }
            true
        });
    })
}

#[cfg(target_os = "windows")]
pub fn configure<R: tauri::Runtime>(
    webview: &tauri::WebviewWindow<R>,
    origin: &str,
) -> tauri::Result<()> {
    use webview2_com::{
        Microsoft::Web::WebView2::Win32::{
            ICoreWebView2, COREWEBVIEW2_PERMISSION_KIND, COREWEBVIEW2_PERMISSION_KIND_CAMERA,
            COREWEBVIEW2_PERMISSION_STATE_ALLOW,
        },
        PermissionRequestedEventHandler,
    };

    let origin = origin.to_owned();
    webview.with_webview(move |platform_webview| {
        let controller = platform_webview.controller();
        let core: ICoreWebView2 = unsafe { controller.CoreWebView2() }
            .expect("WebView2 controller must provide a CoreWebView2 instance");
        let _ = unsafe {
            core.add_PermissionRequested(
                &PermissionRequestedEventHandler::create(Box::new(move |_, args| {
                    let Some(args) = args else { return Ok(()) };
                    let mut kind = COREWEBVIEW2_PERMISSION_KIND::default();
                    args.PermissionKind(&mut kind)?;
                    let mut uri = windows::core::PWSTR::null();
                    args.Uri(&mut uri)?;
                    let uri_text = uri.to_string().unwrap_or_default();
                    windows::Win32::System::Com::CoTaskMemFree(Some(uri.0.cast()));
                    if kind == COREWEBVIEW2_PERMISSION_KIND_CAMERA
                        && is_trusted_loopback_origin(&uri_text, &origin)
                    {
                        // The caller has already performed an explicit user
                        // gesture in the local UI. Windows privacy controls
                        // remain the final OS-level camera gate.
                        args.SetState(COREWEBVIEW2_PERMISSION_STATE_ALLOW)?;
                    }
                    Ok(())
                })),
                &mut Default::default(),
            )
        };
    })
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
pub fn configure<R: tauri::Runtime>(
    _webview: &tauri::WebviewWindow<R>,
    _origin: &str,
) -> tauri::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::is_trusted_loopback_origin;

    #[test]
    fn allows_exact_loopback_origin_and_paths() {
        let origin = "http://localhost:41234";
        assert!(is_trusted_loopback_origin(origin, origin));
        assert!(is_trusted_loopback_origin(
            "http://localhost:41234/",
            origin
        ));
        assert!(is_trusted_loopback_origin(
            "http://localhost:41234/index.html",
            origin
        ));
    }

    #[test]
    fn denies_other_and_prefix_lookalike_origins() {
        let origin = "http://localhost:41234";
        assert!(!is_trusted_loopback_origin(
            "http://localhost:41235/",
            origin
        ));
        assert!(!is_trusted_loopback_origin(
            "http://localhost:412340/",
            origin
        ));
        assert!(!is_trusted_loopback_origin("https://evil.example/", origin));
    }
}
