use crate::{Item, Key};

pub(crate) fn ensure_body_order(entries: &mut [(Key<'_>, Item<'_>)]) {
    // Single pass: once we see a subsection, any subsequent body item means reorder.
    let mut seen_sub = false;
    let mut needs_reorder = false;
    for (_, item) in entries.iter() {
        if item.is_subsection() {
            seen_sub = true;
        } else if seen_sub {
            needs_reorder = true;
            break;
        }
    }

    if !needs_reorder {
        return;
    }

    // Stable sort on a boolean key is a stable partition: body items
    // (false) keep their order and move ahead of subsections (true). This
    // path only runs for programmatically built tables that need reordering.
    entries.sort_by_key(|(_, item)| item.is_subsection());
}
