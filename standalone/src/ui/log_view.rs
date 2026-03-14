/// Scrollable connection log view.
use crate::logging::LogBuffer;
use gtk4::prelude::*;

pub struct LogView {
    pub container: gtk4::Box,
    text_view: gtk4::TextView,
    log_buffer: LogBuffer,
    end_mark: gtk4::TextMark,
}

impl LogView {
    pub fn new(log_buffer: LogBuffer) -> Self {
        let container = gtk4::Box::new(gtk4::Orientation::Vertical, 0);

        let text_view = gtk4::TextView::builder()
            .editable(false)
            .cursor_visible(true)
            .monospace(true)
            .wrap_mode(gtk4::WrapMode::WordChar)
            .top_margin(8)
            .bottom_margin(8)
            .left_margin(8)
            .right_margin(8)
            .focusable(true)
            .build();

        let scrolled = gtk4::ScrolledWindow::builder()
            .child(&text_view)
            .vexpand(true)
            .min_content_height(200)
            .build();

        // Header with clear button
        let header_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        header_box.set_margin_start(8);
        header_box.set_margin_end(8);
        header_box.set_margin_top(4);

        let log_label = gtk4::Label::builder()
            .label("Connection Log")
            .css_classes(["heading"])
            .hexpand(true)
            .xalign(0.0)
            .build();

        let clear_btn = gtk4::Button::builder()
            .icon_name("edit-clear-symbolic")
            .css_classes(["flat"])
            .tooltip_text("Clear Log")
            .build();

        let log_buf_clone = log_buffer.clone();
        let tv_clone = text_view.clone();
        clear_btn.connect_clicked(move |_| {
            log_buf_clone.clear();
            tv_clone.buffer().set_text("");
        });

        header_box.append(&log_label);
        header_box.append(&clear_btn);

        container.append(&header_box);
        container.append(&scrolled);

        // Create a mark at the end of the buffer for reliable auto-scroll
        let buffer = text_view.buffer();
        let end_mark = buffer.create_mark(Some("end"), &buffer.end_iter(), false);

        LogView {
            container,
            text_view,
            log_buffer,
            end_mark,
        }
    }

    /// Refresh the log view from the buffer.
    ///
    /// Only updates the widget when content has changed, to preserve text selection.
    pub fn refresh(&self) {
        let text = self.log_buffer.get_text();
        let buffer = self.text_view.buffer();

        let current = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false);
        if current == text {
            return;
        }

        buffer.set_text(&text);

        // Auto-scroll to bottom using the end mark (reliable after set_text)
        self.text_view.scroll_to_mark(&self.end_mark, 0.0, true, 0.0, 1.0);
    }
}
