//! Servo の `WebViewDelegate` / `ServoDelegate`（iced_servo 由来を最小化）。

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use servo::{
    Cursor, NavigationRequest, RenderingContext, ServoDelegate, ServoError, WebView,
    WebViewDelegate,
};
use tracing::{debug, error, warn};
use url::Url;

type NewWebViewHandler = Rc<dyn Fn(Url)>;

pub(crate) struct DelegateState {
    pub(crate) webview: RefCell<Option<WebView>>,
    pub(crate) rendering_context: RefCell<Option<Rc<dyn RenderingContext>>>,
    pub(crate) pending_popup_webview: RefCell<Option<WebView>>,
    pub(crate) new_webview_handler: RefCell<Option<NewWebViewHandler>>,
    pub(crate) needs_paint: Cell<bool>,
    pub(crate) current_cursor: Cell<Cursor>,
    pub(crate) current_url: RefCell<Option<Url>>,
    pub(crate) current_title: RefCell<Option<String>>,
    pub(crate) status_text: RefCell<Option<String>>,
    pub(crate) load_status: Cell<servo::LoadStatus>,
}

pub(crate) struct WebViewBridge {
    pub(crate) state: Rc<DelegateState>,
}

impl WebViewDelegate for WebViewBridge {
    fn notify_new_frame_ready(&self, _webview: WebView) {
        self.state.needs_paint.set(true);
    }

    fn notify_url_changed(&self, _webview: WebView, url: Url) {
        *self.state.current_url.borrow_mut() = Some(url);
    }

    fn notify_page_title_changed(&self, _webview: WebView, title: Option<String>) {
        *self.state.current_title.borrow_mut() = title;
    }

    fn notify_cursor_changed(&self, _webview: WebView, cursor: Cursor) {
        self.state.current_cursor.set(cursor);
    }

    fn notify_status_text_changed(&self, _webview: WebView, status: Option<String>) {
        *self.state.status_text.borrow_mut() = status;
    }

    fn notify_load_status_changed(&self, _webview: WebView, status: servo::LoadStatus) {
        self.state.load_status.set(status);
    }

    fn notify_crashed(&self, _webview: WebView, reason: String, backtrace: Option<String>) {
        if let Some(bt) = backtrace {
            error!("Servo webview crashed: {reason}\n{bt}");
        } else {
            error!("Servo webview crashed: {reason}");
        }
    }

    fn notify_closed(&self, _webview: WebView) {
        debug!("Servo webview closed");
    }

    fn request_navigation(&self, _webview: WebView, navigation_request: NavigationRequest) {
        navigation_request.allow();
    }

    fn request_create_new(
        &self,
        _parent_webview: WebView,
        request: servo::CreateNewWebViewRequest,
    ) {
        let Some(rc) = self.state.rendering_context.borrow().clone() else {
            warn!("request_create_new before rendering_context was installed");
            return;
        };
        let popup = request
            .builder(rc)
            .delegate(Rc::new(PopupCaptureDelegate {
                state: Rc::clone(&self.state),
            }))
            .build();
        *self.state.pending_popup_webview.borrow_mut() = Some(popup);
    }
}

pub(crate) struct PopupCaptureDelegate {
    pub(crate) state: Rc<DelegateState>,
}

impl WebViewDelegate for PopupCaptureDelegate {
    fn request_navigation(&self, _webview: WebView, navigation_request: NavigationRequest) {
        let url = navigation_request.url.clone();
        navigation_request.deny();
        let _ = self.state.pending_popup_webview.borrow_mut().take();
        if let Some(handler) = self.state.new_webview_handler.borrow().as_ref() {
            handler(url);
        }
    }
}

pub(crate) struct ServoBridge;

impl ServoDelegate for ServoBridge {
    fn notify_error(&self, error: ServoError) {
        warn!("Servo engine error: {error:?}");
    }
}
