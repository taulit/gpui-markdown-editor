//! The press feedback of a checkbox (plan 04: scale 0.85 on a spring).

use std::time::Instant;

use super::MarkdownEditor;
use crate::model::BlockId;
use gpui::{App, Context, SpringConfig, SpringState, Window};

/// The scale of a pressed checkbox.
pub(super) const PRESSED: f32 = 0.85;

/// Plan 04's spring: stiffness 300, damping 26.
const SPRING: SpringConfig = SpringConfig::new(300., 26., 1.);

/// Close enough to the target to stop animating.
const EPSILON: f32 = 0.002;

/// A checkbox springing towards [`PRESSED`] while the mouse button is held
/// on it, and back to 1 after.
#[derive(Clone, Copy, Debug)]
pub(super) struct CheckboxPress {
    pub id: BlockId,
    spring: SpringState,
    target: f32,
    updated: Instant,
}

impl MarkdownEditor {
    /// The mouse went down on the checkbox of `id`.
    pub(super) fn press_checkbox(&mut self, id: BlockId, cx: &App) {
        self.checkbox_press = Some(CheckboxPress {
            id,
            spring: SpringState {
                position: 1.,
                velocity: 0.,
            },
            target: PRESSED,
            updated: cx.background_executor().now(),
        });
    }

    /// The mouse button was released.
    pub(super) fn release_checkbox(&mut self, cx: &mut Context<Self>) {
        if let Some(press) = &mut self.checkbox_press {
            press.target = 1.;
            // No frames are drawn while the spring rests at the press scale,
            // so the next step would otherwise cover the whole hold.
            press.updated = cx.background_executor().now();
            cx.notify();
        }
    }

    /// Advances the spring to now; called once per frame. Requests the next
    /// frame only while the spring moves, so a settled press costs nothing.
    pub(super) fn step_checkbox_press(&mut self, window: &mut Window, cx: &App) {
        let Some(press) = &mut self.checkbox_press else {
            return;
        };
        if cx.reduce_motion() {
            self.checkbox_press = None;
            return;
        }
        let now = cx.background_executor().now();
        let dt = now.saturating_duration_since(press.updated).as_secs_f32();
        press.spring = SPRING.step(press.spring, press.target, dt);
        press.updated = now;
        if SPRING.is_settled(press.spring, press.target, EPSILON) {
            press.spring = SpringState {
                position: press.target,
                velocity: 0.,
            };
            if press.target == 1. {
                self.checkbox_press = None;
            }
        } else {
            window.request_animation_frame();
        }
    }

    /// The checkbox drawn scaled, and its scale.
    pub(super) fn checkbox_scale(&self) -> Option<(BlockId, f32)> {
        self.checkbox_press.map(|p| (p.id, p.spring.position))
    }
}
