use crate::ai::{Ai, Features};
use std::path::PathBuf;
use tokio::time::{Duration, sleep};

use crate::{
    input::Args,
    scheduler::{Schedule, ScheduleElement, Scheduler},
};

pub async fn sunny(args: Args, mut ai: impl Ai, dynamic_schedule_interval: u64) {
    let timer_duration = Duration::from_secs(dynamic_schedule_interval);
    let cores = args.cores.unwrap_or(2);
    let scheduler = Scheduler::new(args.clone());

    scheduler.apply_schedule(static_schedule(cores)); // TODO: Maybe do this in another thread
    scheduler.listen_for_results();

    let mut timer = sleep(timer_duration);
    let fnz = convert_mzn_to_fzn(args.model, args.data);
    let features = fzn_to_features(fnz);

    loop {
        timer.await;
        let schedule = ai.schedule(&features, cores);
        scheduler.apply_schedule(schedule);
        timer = sleep(timer_duration);
    }
}

fn static_schedule(cores: usize) -> Schedule {
    vec![
        ScheduleElement::new("gecode".to_string(), cores / 2),
        ScheduleElement::new("coinbc".to_string(), cores / 2),
    ]
}

fn convert_mzn_to_fzn(model: PathBuf, data: Option<PathBuf>, solver_name: &str) -> PathBuf {
    todo!()
}

fn mzn_to_features(fnz: PathBuf) -> Features {
    todo!()
}
