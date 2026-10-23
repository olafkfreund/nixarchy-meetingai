//! Shared, restrained motion: no overshoot and no timers after settling.
use gpui::{Animation, AnimationExt, ElementId, IntoElement, ParentElement, Styled, div, px};
use std::time::{Duration, Instant};

pub const PAGE_MS: u64 = 200;
pub const POPUP_MS: u64 = 160;
pub const LAYOUT_MS: u64 = 240;

pub fn ease(t: f32) -> f32 {
    1.0 - (1.0 - t.clamp(0.0, 1.0)).powi(3)
}

/// Entrance effects keep their identity while content or text is edited.
pub fn reveal(
    content: impl IntoElement + 'static,
    id: impl Into<ElementId>,
    reduced: bool,
    popup: bool,
) -> gpui::AnyElement {
    let panel = div().relative().w_full().min_w(px(0.0)).child(content);
    if reduced {
        return panel.into_any_element();
    }
    panel
        .with_animation(
            id,
            Animation::new(Duration::from_millis(if popup {
                POPUP_MS
            } else {
                PAGE_MS
            }))
            .with_easing(ease),
            move |panel, t| {
                panel
                    .opacity(t)
                    .top(px((1.0 - t) * if popup { 4.0 } else { 6.0 }))
            },
        )
        .into_any_element()
}

/// Retarget from the current value, so rapid mode changes never jump back.
pub struct Transition {
    from: f32,
    target: f32,
    started: Instant,
    duration: Duration,
}
impl Transition {
    pub fn new(value: f32) -> Self {
        Self {
            from: value,
            target: value,
            started: Instant::now(),
            duration: Duration::from_millis(LAYOUT_MS),
        }
    }
    fn value(&self, now: Instant) -> f32 {
        let t =
            now.saturating_duration_since(self.started).as_secs_f32() / self.duration.as_secs_f32();
        self.from + (self.target - self.from) * ease(t)
    }
    pub fn step(&mut self, target: f32, reduced: bool, now: Instant) -> (f32, bool) {
        if reduced {
            self.from = target;
            self.target = target;
            return (target, false);
        }
        if self.target != target {
            self.from = self.value(now);
            self.target = target;
            self.started = now;
        }
        let active =
            self.from != self.target && now.saturating_duration_since(self.started) < self.duration;
        (if active { self.value(now) } else { self.target }, active)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reversing_preserves_position_and_reduced_motion_settles_immediately() {
        let now = Instant::now();
        let mut transition = Transition::new(0.0);
        transition.step(1.0, false, now);
        let halfway = now + Duration::from_millis(100);
        let before = transition.step(1.0, false, halfway).0;
        let after = transition.step(0.0, false, halfway).0;
        assert!((before - after).abs() < 0.0001);
        assert_eq!(transition.step(1.0, true, halfway), (1.0, false));
        assert_eq!(
            transition.step(1.0, false, halfway + Duration::from_secs(1)),
            (1.0, false)
        );
    }
}
