use crate::scheduler::{Schedule, ScheduleElement};
pub type Features = Vec<i64>;

pub trait Ai {
    fn schedule(&mut self, features: &Features, cores: usize) -> Schedule;
}

pub struct SimpleAi {}

impl Ai for SimpleAi {
    fn schedule(&mut self, features: &Features, cores: usize) -> Schedule {
        vec![
            ScheduleElement::new("gecode".to_string(), cores / 2),
            ScheduleElement::new("coinbc".to_string(), cores / 2),
        ]
    }
}
