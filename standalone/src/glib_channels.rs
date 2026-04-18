//! Reactive message dispatch for worker threads.
//!
//! Provides a wrapper that allows worker threads to send messages to the
//! GTK main thread reactively using `glib::idle_add_once`.

use gtk4::glib;
use std::sync::Arc;

/// A sender that dispatches messages to the GTK main thread reactively.
///
/// Each `send()` call schedules a callback on the GTK main loop.
/// `Send + Sync + Clone` is auto-derived from the `Arc<dyn Fn + Send + Sync>` callback.
pub struct GlibSender<Msg>
where
    Msg: Send + 'static,
{
    callback: Arc<dyn Fn(Msg) + Send + Sync>,
}

impl<Msg> Clone for GlibSender<Msg>
where
    Msg: Send + 'static,
{
    fn clone(&self) -> Self {
        GlibSender {
            callback: self.callback.clone(),
        }
    }
}

impl<Msg> GlibSender<Msg>
where
    Msg: Send + 'static,
{
    /// Create a new GlibSender with a callback function.
    pub fn new<F>(callback: F) -> Self
    where
        F: Fn(Msg) + Send + Sync + 'static,
    {
        GlibSender {
            callback: Arc::new(callback),
        }
    }

    /// Send a message to the GTK main thread.
    pub fn send(&self, msg: Msg) {
        let callback = self.callback.clone();
        glib::idle_add_once(move || {
            callback(msg);
        });
    }
}
