use super::Item;
use super::array::Array;
use super::table::Table;

impl<'de> Item<'de> {
    /// Access projected item from source computed in reprojection.
    pub(crate) fn projected<'a>(&self, inputs: &[&'a Item<'a>]) -> Option<&'a Item<'a>> {
        let index = self.meta().projected_index();
        inputs.get(index as usize).copied()
    }
    pub(crate) fn set_reprojected_to_none(&mut self) {
        self.meta_mut().set_reprojected_to_none();
    }
    pub(crate) fn set_reprojected_index(&mut self, index: usize) -> bool {
        self.meta_mut().set_reprojected_index(index)
    }
    /// Marks this item as a full match during reprojection.
    pub(crate) fn set_reprojected_full_match(&mut self) {
        self.meta_mut().set_reprojected_full_match();
    }
    /// Returns whether this item was marked as a full match during reprojection.
    pub(crate) fn is_reprojected_full_match(&self) -> bool {
        self.meta().is_reprojected_full_match()
    }

    /// Returns `true` if this item is emitted as a subsection rather than
    /// as part of the body: `[header]` tables, implicit tables, and
    /// `[[array-of-tables]]`.
    #[inline]
    pub(crate) fn is_subsection(&self) -> bool {
        self.has_header_bit() || self.is_implicit_table() || self.is_aot()
    }

    #[inline]
    #[cfg(test)]
    pub(crate) fn set_flag(&mut self, flag: u32) {
        self.meta_mut().set_flag(flag);
    }
}

impl<'de> Table<'de> {
    /// Disables source-position reordering for this table's immediate entries
    /// during emission. Non-recursive: child tables are unaffected.
    pub fn set_ignore_source_order(&mut self) {
        self.meta_mut().set_ignore_source_order();
    }

    /// Returns `true` if source-position reordering is disabled for this table.
    #[must_use]
    pub fn ignore_source_order(&self) -> bool {
        self.meta().ignore_source_order()
    }

    /// Disables copying structural styles (TableStyle/ArrayStyle) from source
    /// during reprojection for this table's immediate entries. Key spans and
    /// reprojection indices are still copied. Non-recursive.
    pub fn set_ignore_source_style(&mut self) {
        self.meta_mut().set_ignore_source_style();
    }

    /// Returns `true` if source-style copying is disabled for this table.
    #[must_use]
    pub fn ignore_source_style(&self) -> bool {
        self.meta().ignore_source_style()
    }

    /// Returns `true` if this table has automatic style resolution pending.
    #[must_use]
    pub fn is_auto_style(&self) -> bool {
        self.meta().is_auto_style()
    }
}

impl<'de> Array<'de> {
    /// Returns `true` if this array has automatic style resolution pending.
    #[must_use]
    pub fn is_auto_style(&self) -> bool {
        self.meta().is_auto_style()
    }

    /// Returns `true` if this inline array should be emitted in multiline
    /// format with one element per line.
    #[must_use]
    pub fn is_expanded(&self) -> bool {
        self.meta().is_expanded()
    }

    /// Marks this inline array for multiline emission.
    pub fn set_expanded(&mut self) {
        self.meta_mut().set_expanded();
    }

    /// Clears the multiline emission hint.
    pub fn clear_expanded(&mut self) {
        self.meta_mut().clear_expanded();
    }
}
