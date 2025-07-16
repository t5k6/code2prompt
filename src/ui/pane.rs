#![cfg(feature = "tui")]

//! Defines a common trait for interactive, navigable panes in the TUI.

/// A trait for TUI panes that contain a list of items that can be
/// navigated and selected.
pub trait NavigablePane {
    /// Moves the selection cursor to the next item in the list.
    /// Implementations should handle wrapping from the last to the first item.
    fn next(&mut self);

    /// Moves the selection cursor to the previous item in the list.
    /// Implementations should handle wrapping from the first to the last item.
    fn previous(&mut self);

    /// Toggles the selection state of the currently highlighted item.
    ///
    /// # Returns
    ///
    /// `true` if the selection state was changed and a data recalculation
    /// is likely required. `false` otherwise.
    fn toggle_current_selection(&mut self) -> bool;
}
