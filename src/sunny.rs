use crate::ai::Ai;
use crate::fzn_to_features::fzn_to_features;
use crate::mzn_to_fzn::convert_mzn_to_fzn;
use tokio::time::{Duration, sleep};

use crate::{
    input::Args,
    scheduler::{Schedule, ScheduleElement, Scheduler},
};

const FEATURES_SOLVER: &str = "gecode";

pub async fn sunny(args: Args, mut ai: impl Ai, dynamic_schedule_interval: u64) {
    let timer_duration = Duration::from_secs(dynamic_schedule_interval);
    let cores = args.cores.unwrap_or(2);
    let mut scheduler = Scheduler::new(args.clone());

    apply_schedule(&mut scheduler, static_schedule(cores))
        .await
        .expect("if we fail to apply the static schedule, we can't recover"); // TODO: Maybe do this in another thread

    let mut timer = sleep(timer_duration);
    let fzn = convert_mzn_to_fzn(&args.model, args.data.as_deref(), FEATURES_SOLVER)
        .await
        .expect("failed to initially convert .mzn to .fzn");
    // let features = fzn_to_features(&fzn)
    //     .await
    //     .expect("if we fail to get features, we can't run the AI and thus can't recover");

    loop {
        timer.await;
        // let schedule = ai.schedule(&vec![], cores);
        // apply_schedule(&mut scheduler, schedule).await;
        timer = sleep(timer_duration);
    }
}

async fn apply_schedule(
    scheduler: &mut Scheduler,
    schedule: Schedule,
) -> Result<(), crate::scheduler::Error> {
    scheduler.stop_all_solvers().await.unwrap();
    scheduler.start_solvers(schedule).await.unwrap();
    Ok(())
}

fn static_schedule(cores: usize) -> Schedule {
    vec![
        ScheduleElement::new("picat".to_string(), cores / 10, 0),
        ScheduleElement::new("gecode".to_string(), cores / 10, 0),
        ScheduleElement::new("cp-sat".to_string(), cores / 10, 0),
        ScheduleElement::new("chuffed".to_string(), cores / 10, 0),
        ScheduleElement::new("coinbc".to_string(), cores / 10, 0),
        ScheduleElement::new("yuck".to_string(), cores / 10, 0),
        ScheduleElement::new("xpress".to_string(), cores / 10, 0),
        ScheduleElement::new("scip".to_string(), cores / 10, 0),
        ScheduleElement::new("highs".to_string(), cores / 10, 0),
        ScheduleElement::new("gurobi".to_string(), cores / 10, 0),
        // ScheduleElement::new("coinbc".to_string(), cores / 2, 1),
    ]
}
