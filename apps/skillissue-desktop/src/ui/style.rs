// Shared desktop density. Icon-only overlay actions use the compact variant.
use super::*;

pub(super) const CONTROL_HEIGHT: f32 = 32.;
pub(super) const COMPACT_HEIGHT: f32 = 24.;
pub(super) const CONTROL_RADIUS: f32 = 6.;
pub(super) const OVERLAY_COLLAPSED_HEIGHT: f32 = 256.;

pub(super) fn action(id: impl Into<ElementId>) -> Button {
    Button::new(id)
        .h(px(CONTROL_HEIGHT))
        .px_3()
        .rounded(px(CONTROL_RADIUS))
}
