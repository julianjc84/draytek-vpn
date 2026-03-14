/// Reactive message dispatch for worker threads.
///
/// Provides a wrapper that allows worker threads to send messages to the
/// GTK main thread reactively using `glib::idle_add_once`.
use gtk4::glib;
use std::marker::PhantomData;
use std::sync::Arc;

/// A sender that dispatches messages to the GTK main thread reactively.
///
/// Each `send()` call schedules a callback on the GTK main loop.
/// This type is `Send + Sync + Clone`.
pub struct GlibSender<Msg>
where
    Msg: Send + 'static,
{
    callback: Arc<dyn Fn(Msg) + Send + Sync>,
    _phantom: PhantomData<fn(Msg)>,
}

impl<Msg> Clone for GlibSender<Msg>
where
    Msg: Send + 'static,
{
    fn clone(&self) -> Self {
        GlibSender {
            callback: self.callback.clone(),
            _phantom: PhantomData,
        }
    }
}

// Safety: The callback is Send + Sync, so GlibSender is Send + Sync
unsafe impl<Msg: Send + 'static> Send for GlibSender<Msg> {}
unsafe impl<Msg: Send + 'static> Sync for GlibSender<Msg> {}

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
            _phantom: PhantomData,
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
