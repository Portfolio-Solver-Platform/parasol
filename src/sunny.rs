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
    let mut scheduler = Scheduler::new(args.clone()).expect("Failed to create scheduler");

    apply_schedule(&mut scheduler, static_schedule(cores))
        .await
        .expect("if we fail to apply the static schedule, we can't recover"); // TODO: Maybe do this in another thread

    let mut timer = sleep(timer_duration);
    let fzn = convert_mzn_to_fzn(
        &args.model,
        args.data.as_deref(),
        FEATURES_SOLVER,
        args.debug_verbosity,
    )
    .await
    .expect("failed to initially convert .mzn to .fzn");
    // let features = fzn_to_features(&fzn)
    //     .await
    //     .expect("if we fail to get features, we can't run the AI and thus can't recover");

    loop {
        timer.await;
        // let schedule = ai.schedule(&vec![], cores);
        // apply_schedule(&mut scheduler, schedule).await;
        // scheduler.stop_all_solvers().await.unwrap();

        // scheduler.suspend_all_solvers().await.unwrap();
        // scheduler.resume_all_solvers().await.unwrap();

        // scheduler.resume_solver(1).await.unwrap();
        // scheduler.resume_solver(2).await.unwrap();
        // scheduler.resume_solver(1).await.unwrap();
        // scheduler.resume_solver(1).await.unwrap();
        // scheduler.resume_solver(1).await.unwrap();

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
    // let solvers = vec![
    //     "picat".to_string(),
    //     "gecode".to_string(),
    //     "cp-sat".to_string(),
    //     "chuffed".to_string(),
    //     "coinbc".to_string(),
    //     "yuck".to_string(),
    // ];
    // let mut schedule = vec![];
    // for (i, solver) in solvers.into_iter().cycle().take(100).enumerate() {
    //     schedule.push(ScheduleElement::new(i, solver, 1));
    // }
    // schedule
    vec![
        ScheduleElement::new(0, "picat".to_string(), 1),
        ScheduleElement::new(1, "gecode".to_string(), 1),
        ScheduleElement::new(2, "cp-sat".to_string(), 1),
        ScheduleElement::new(3, "chuffed".to_string(), 1),
        ScheduleElement::new(4, "coinbc".to_string(), 1),
        ScheduleElement::new(5, "yuck".to_string(), 1),
        // ScheduleElement::new(6, "xpress".to_string(), cores / 10),
        // ScheduleElement::new(7, "scip".to_string(), cores / 10),
        ScheduleElement::new(8, "highs".to_string(), cores / 10),
        // ScheduleElement::new(9, "gurobi".to_string(), cores / 10),
        // ScheduleElement::new(10, "coinbc".to_string(), cores / 2),
    ]
}
