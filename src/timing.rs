#[derive(Clone, PartialEq)]
pub enum EffectDuration {
    Immediate,
    Persistent(Option<SmallTimer>),
    Continuous(Option<SmallTimer>),
    Repeating(RepeatingSmallTimer, Option<SmallTimer>),
}

#[derive(Clone, PartialEq)]
pub struct SmallTimer {
    pub(crate) remaining: f32,
}

impl SmallTimer {
    pub(crate) fn tick(&mut self, secs: f32) {
        self.remaining -= secs;
    }

    pub(crate) fn finished(&self) -> bool {
        self.remaining <= 0.
    }

    pub fn set_duration(&mut self, timer: impl Into<SmallTimer>) {
        self.remaining = timer.into().remaining;
    }
}

impl From<f32> for SmallTimer {
    fn from(value: f32) -> Self {
        Self { remaining: value }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub struct RepeatingSmallTimer {
    period: f32,
    pub(crate) remaining: f32,
    triggered: bool,
}

impl RepeatingSmallTimer {
    pub fn new(period: f32, initial_delay: f32) -> Self {
        Self {
            period,
            remaining: initial_delay,
            triggered: false,
        }
    }

    pub(crate) fn tick(&mut self, secs: f32) {
        self.remaining -= secs;
        if self.remaining <= 0. {
            self.remaining += self.period;
            self.triggered = true;
            self.remaining = f32::max(self.remaining, 0.);
        } else {
            self.triggered = false;
        }
    }

    pub(crate) fn just_triggered(&self) -> bool {
        self.triggered
    }
}
