use rustyline::{Cmd, ConditionalEventHandler, Event, EventContext, RepeatCount};

/// Clear terminal scrollback before the editor clears/repaints its own layout.
/// Using the editor's ClearScreen command preserves both input and cursor.
pub(super) struct ClearScreenBinding;

impl ConditionalEventHandler for ClearScreenBinding {
    fn handle(&self, _: &Event, _: RepeatCount, _: bool, _: &EventContext<'_>) -> Option<Cmd> {
        // The editor's native fallback runs even if VT is unavailable or the
        // write fails; editor errors then propagate through readline as usual.
        let _ = crate::platform::console::clear_screen(&mut std::io::stdout());
        Some(Cmd::ClearScreen)
    }
}
