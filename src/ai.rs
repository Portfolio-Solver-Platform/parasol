use crate::scheduler::{Schedule, ScheduleElement};
pub type Features = Vec<f32>;

pub trait Ai {
    fn schedule(&mut self, features: &Features, cores: usize) -> Schedule;
}

pub struct SimpleAi {}

impl Ai for SimpleAi {
    fn schedule(&mut self, features: &Features, cores: usize) -> Schedule {
        vec![
            ScheduleElement::new("gecode".to_string(), cores / 2, 1),
            // ScheduleElement::new("coinbc".to_string(), cores / 2, 2),
            // ScheduleElement::new("coingbc".to_string(), cores / 2, 2),
        ]
    }
}
